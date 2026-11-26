use std::{
    collections::{BTreeMap, BTreeSet},
    time::SystemTime,
};

use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::json;

use crate::{
    archive::zip_reader::ZipEntry,
    contracts::IndexerBatchUpdatedEvent,
    error::IpcError,
    hasher::fingerprint,
    scanner::{
        attr_guard::unix_seconds,
        grouping::{is_image, parent_path, technical, Grouping},
        title::display_title,
        walker::{InventoryEntry, WalkCoverage},
    },
};

use super::Database;

pub(crate) fn write_inventory_batch(
    db: &Database,
    source_id: i64,
    entries: &[InventoryEntry],
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let (root, source_name) = source(&tx, source_id)?;
        let mut folders = BTreeSet::new();
        let mut models = BTreeSet::new();
        for entry in entries {
            match entry {
                InventoryEntry::Directory { rel_path, name, opaque } => {
                    // A new technical child may disappear into its parent pack at grouping.
                    let visible = *opaque || parent_path(rel_path).is_empty() || !technical(name);
                    let id = upsert_folder(&tx, source_id, rel_path,
                        if rel_path.is_empty() { &source_name } else { name }, visible)?;
                    folders.insert(id);
                }
                InventoryEntry::UnobservedLink { .. } => {}
                InventoryEntry::File { rel_path, name, ext, metadata } => {
                    let folder = folder_id(&tx, source_id, parent_path(rel_path))?;
                    if ext == "zip" {
                        reveal_folder(&tx, folder)?;
                    }
                    let uri = local_uri(&root, rel_path);
                    let cloud = metadata.flags.is_placeholder();
                    let storage = if cloud { "cloud_placeholder" } else { "local_resident" };
                    let pass = if cloud { 2 } else { 1 };
                    let size = i64::try_from(metadata.size_bytes).map_err(|_| IpcError::InternalError)?;
                    let existing: Option<(Option<i64>, Option<i64>)> = tx.prepare_cached(
                        "SELECT model_id, folder_id FROM files WHERE uri=?1",
                    )?.query_row([&uri], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
                    let (model, folder) = match existing {
                        Some((model, current_folder)) => (model, current_folder.unwrap_or(folder)),
                        None if is_image(ext) => (None, folder),
                        None => {
                            let kind = if ext == "zip" { "zip_archive" } else { "single_file" };
                            (Some(upsert_model(&tx, source_id, folder, kind, &uri, name, ext, cloud)?), folder)
                        }
                    };
                    let id: i64 = tx.prepare_cached(
                        "INSERT INTO files (model_id, source_id, folder_id, uri, rel_path, filename, ext,
                                           storage_state, size_bytes, mtime_unix, index_pass)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10)
                         ON CONFLICT(uri) DO UPDATE SET folder_id = excluded.folder_id,
                             storage_state = excluded.storage_state, size_bytes = excluded.size_bytes,
                             mtime_unix = excluded.mtime_unix,
                             index_pass = CASE WHEN excluded.storage_state='local_resident'
                                 AND files.storage_state=excluded.storage_state AND files.size_bytes=excluded.size_bytes
                                 AND files.mtime_unix=excluded.mtime_unix AND files.content_hash IS NOT NULL
                                 THEN files.index_pass ELSE excluded.index_pass END,
                             content_hash = CASE WHEN excluded.storage_state='local_resident'
                                 AND files.storage_state=excluded.storage_state AND files.size_bytes=excluded.size_bytes
                                 AND files.mtime_unix=excluded.mtime_unix THEN files.content_hash END
                         RETURNING id",
                    )?.query_row(params![model, source_id, folder, uri, name, ext, storage, size, metadata.mtime_unix, pass], |r| r.get(0))?;
                    sync_file_fts(&tx, id, model, name, rel_path, ext)?;
                    if let Some(model) = model {
                        if existing.is_none() {
                            tx.prepare_cached("UPDATE models SET primary_file_id=?2 WHERE id=?1")?.execute(params![model, id])?;
                        }
                        models.insert(model);
                    }
                    folders.insert(folder);
                }
            }
        }
        for &model in &models {
            sync_model(&tx, &root, model)?;
        }
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent {
            source_id,
            updated_folder_ids: folders.into_iter().collect(),
            updated_model_ids: models.into_iter().collect(),
        })
    })
}

pub(crate) fn write_groups(
    db: &Database,
    source_id: i64,
    grouping: &Grouping<'_>,
    entries: &[InventoryEntry],
    history: &ModelHistory,
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let (root, source_name) = source(&tx, source_id)?;
        let mut model_ids = BTreeSet::new();
        for model in &grouping.models {
            let folder = folder_id(&tx, source_id, model.folder)?;
            let uri = local_uri(&root, model.path);
            let name = if model.path.is_empty() { &source_name } else { model.name };
            let primary = model.members[0];
            let id = upsert_model(&tx, source_id, folder, model.kind, &uri, name, primary.ext,
                                  model.members.iter().any(|f| f.cloud))?;
            for member in &model.members {
                let member_uri = local_uri(&root, member.path);
                let rel = if model.kind == "folder_pack" {
                    relative_to(model.path, member.path)
                } else { member.name };
                let file_id: i64 = tx.prepare_cached(
                    "UPDATE files SET model_id=?2, folder_id=?3, rel_path=?4 WHERE uri=?1 RETURNING id",
                )?.query_row(params![member_uri, id, folder, rel], |r| r.get(0))?;
                sync_file_fts(&tx, file_id, Some(id), member.name, member.path, member.ext)?;
                if member.path == primary.path {
                    tx.prepare_cached("UPDATE models SET primary_file_id=?2 WHERE id=?1")?.execute(params![id, file_id])?;
                }
            }
            model_ids.insert(id);
        }
        for member in &grouping.orphans {
            let folder = folder_id(&tx, source_id, parent_path(member.path))?;
            let uri = local_uri(&root, member.path);
            let file_id: i64 = tx.prepare_cached(
                "UPDATE files SET model_id=NULL, folder_id=?2, rel_path=?3 WHERE uri=?1 RETURNING id",
            )?.query_row(params![uri, folder, member.name], |r| r.get(0))?;
            sync_file_fts(&tx, file_id, None, member.name, member.path, member.ext)?;
        }
        for &id in &model_ids {
            sync_model(&tx, &root, id)?;
        }
        // Models emptied by regrouping need not survive until move reconciliation unless historical.
        let empty = tx.prepare_cached(
            "SELECT id,canonical_uri FROM models m WHERE source_id=?1 AND container_kind='single_file'
             AND NOT EXISTS (SELECT 1 FROM files WHERE model_id=m.id)",
        )?.query_map([source_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
          .collect::<Result<Vec<_>, _>>()?;
        for (id, uri) in empty {
            if !history.contains_key(source_relative(&root, &uri)?) {
                delete_model(&tx, id)?;
            }
        }
        let mut visibility = tx.prepare_cached(
            "UPDATE folders SET is_visible=?3 WHERE source_id=?1 AND rel_path=?2 AND is_visible<>?3",
        )?;
        for entry in entries {
            if let InventoryEntry::Directory { rel_path, .. } = entry {
                visibility.execute(params![source_id, rel_path, !grouping.folded.contains(rel_path.as_str())])?;
            }
        }
        drop(visibility);
        let folder_ids = refresh_folders(&tx, source_id)?;
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent { source_id, updated_model_ids: model_ids.into_iter().collect(), updated_folder_ids: folder_ids })
    })
}

pub(crate) struct FileAnalysis<'a> {
    pub path: &'a str,
    pub hash: Option<String>,
    pub zip: Option<Vec<ZipEntry>>,
    pub complete: bool,
}

// Cache presence means the Pass 1 size/mtime check kept a resident hash.
// A non-null count also means the rest of Pass 2 completed.
pub(crate) fn cached_analysis(
    db: &Database,
    source_id: i64,
) -> Result<BTreeMap<String, Option<i64>>, IpcError> {
    db.with_reader(|conn| {
        let root: String = conn.query_row("SELECT root_uri FROM sources WHERE id=?1", [source_id], |r| r.get(0))?;
        let prefix = local_prefix(&root);
        let mut stmt = conn.prepare(
            "SELECT f.uri, CASE WHEN f.index_pass=2 THEN CASE WHEN f.ext='zip' THEN
               (SELECT count(*) FROM files entry WHERE entry.parent_archive_id=f.id) ELSE 0 END END
             FROM files f WHERE f.source_id=?1 AND f.storage_state='local_resident' AND f.content_hash IS NOT NULL",
        )?;
        let mut cached = BTreeMap::new();
        let mut rows = stmt.query([source_id])?;
        while let Some(row) = rows.next()? {
            let uri: String = row.get(0)?;
            if let Some(path) = uri.strip_prefix(prefix.as_ref()) {
                cached.insert(path.to_owned(), row.get(1)?);
            }
        }
        Ok(cached)
    })
}

pub(crate) fn write_analysis(
    db: &Database,
    source_id: i64,
    batch: &[FileAnalysis<'_>],
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let (root, _) = source(&tx, source_id)?;
        let mut models = Vec::new();
        for FileAnalysis { path, hash, zip, complete } in batch {
            let parent_uri = local_uri(&root, path);
            tx.prepare_cached(
                "UPDATE files SET content_hash=coalesce(?2, content_hash),
                   index_pass=CASE WHEN ?3 THEN 2 ELSE index_pass END WHERE uri=?1",
            )?.execute(params![parent_uri, hash, complete])?;
            let Some(entries) = zip else { continue };
            let (parent, model, folder): (i64, i64, i64) = tx.prepare_cached(
                "SELECT id, model_id, folder_id FROM files WHERE uri=?1",
            )?.query_row([&parent_uri], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            let mut previous = tx.prepare_cached("SELECT id, rel_path FROM files WHERE parent_archive_id=?1")?;
            let mut previous = previous.query_map([parent], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(0)?)))?.collect::<Result<BTreeMap<_, _>, _>>()?;
            for entry in entries {
                // Escape percent and the URI delimiter in the parent to prevent ambiguous zip IDs.
                let archive_uri = parent_uri.replace('%', "%25").replace('#', "%23");
                let uri = format!("zip://{archive_uri}#{}", entry.path);
                let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
                let id: i64 = tx.prepare_cached(
                    "INSERT INTO files (model_id, source_id, folder_id, parent_archive_id, uri, rel_path, filename, ext,
                        storage_state, size_bytes, zip_entry_csize, zip_entry_offset, zip_entry_method, index_pass)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'zip_entry', ?9, ?10, ?11, ?12, 2)
                     ON CONFLICT(uri) DO UPDATE SET folder_id=excluded.folder_id,
                       size_bytes=excluded.size_bytes, zip_entry_csize=excluded.zip_entry_csize,
                       zip_entry_offset=excluded.zip_entry_offset, zip_entry_method=excluded.zip_entry_method
                     RETURNING id",
                )?.query_row(params![model, source_id, folder, parent, uri, entry.path, name, entry.ext,
                                     entry.size, entry.compressed_size, entry.offset, entry.method], |r| r.get(0))?;
                if previous.get(&entry.path) == Some(&id) {
                    previous.remove(&entry.path);
                }
                sync_file_fts(&tx, id, Some(model), name, &format!("{path}/{}", entry.path), &entry.ext)?;
            }
            for id in previous.into_values() {
                tx.prepare_cached("DELETE FROM files_fts WHERE rowid=?1")?.execute([id])?;
                tx.prepare_cached("DELETE FROM files WHERE id=?1")?.execute([id])?;
            }
            sync_model(&tx, &root, model)?;
            models.push(model);
        }
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent { source_id, updated_model_ids: models, updated_folder_ids: Vec::new() })
    })
}

pub(crate) struct HistoricalModel {
    fingerprint: Option<String>,
    pub pack: bool,
    cloud: bool,
}

pub(crate) type ModelHistory = BTreeMap<String, HistoricalModel>;

pub(crate) fn scan_history(db: &Database, source_id: i64) -> Result<ModelHistory, IpcError> {
    db.with_reader(|conn| {
        let sql = "SELECT root_uri FROM sources WHERE id=?1";
        let root: String = conn.query_row(sql, [source_id], |r| r.get(0))?;
        let mut stmt = conn.prepare("SELECT canonical_uri,fingerprint,container_kind='folder_pack',has_cloud_placeholder FROM models WHERE source_id=?1")?;
        let mut rows = stmt.query([source_id])?;
        let mut history = ModelHistory::new();
        while let Some(row) = rows.next()? {
            let uri: String = row.get(0)?;
            let path = source_relative(&root, &uri)?;
            history.insert(path.to_owned(), HistoricalModel { fingerprint: row.get(1)?, pack: row.get(2)?, cloud: row.get(3)? });
        }
        Ok(history)
    })
}

pub(crate) fn legacy_root_children(
    db: &Database,
    source_id: i64,
) -> Result<BTreeSet<String>, IpcError> {
    db.with_reader(|conn| {
        let mut rows = conn.prepare(
            "SELECT DISTINCT substr(f.rel_path,1,instr(f.rel_path,'/')-1)
             FROM sources s JOIN models m ON m.canonical_uri=s.root_uri
             JOIN files f ON f.model_id=m.id
             WHERE s.id=?1 AND m.container_kind='folder_pack'
               AND f.parent_archive_id IS NULL AND instr(f.rel_path,'/')>0",
        )?;
        let names = rows.query_map([source_id], |r| r.get::<_, String>(0))?;
        let mut children = BTreeSet::new();
        for name in names {
            let name = name?;
            if technical(&name) {
                children.insert(WalkCoverage::key(&name));
            }
        }
        Ok(children)
    })
}

struct ScanFile {
    id: i64,
    uri: String,
    hash: Option<String>,
    size: i64,
    cloud: bool,
}

pub(crate) fn finish_scan(
    db: &Database,
    source_id: i64,
    entries: &[InventoryEntry],
    grouping: &Grouping<'_>,
    history: &ModelHistory,
    coverage: &WalkCoverage,
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let (root, _) = source(&tx, source_id)?;
        let mut live_files = BTreeSet::new();
        let mut live_folders = BTreeSet::new();
        for entry in entries {
            match entry {
                InventoryEntry::File { rel_path, .. } => {
                    live_files.insert(rel_path.as_str());
                }
                InventoryEntry::Directory { rel_path, .. } => {
                    live_folders.insert(rel_path.as_str());
                }
                _ => {}
            }
        }
        let mut files = read_scan_files(&tx, source_id)?;
        let live_models: BTreeSet<_> = grouping.models.iter().map(|model| model.path).collect();
        let mut missing = BTreeMap::<_, Vec<_>>::new();
        let mut new = BTreeMap::<String, Vec<i64>>::new();
        let mut active = BTreeSet::new();
        let mut resync = BTreeSet::new();
        let mut prune_files = Vec::new();
        let mut obsolete = BTreeSet::new();
        let mut model_rows =
            tx.prepare("SELECT id,canonical_uri,container_kind FROM models WHERE source_id=?1")?;
        let mut rows = model_rows.query([source_id])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let uri: String = row.get(1)?;
            let path = source_relative(&root, &uri)?;
            let kind: String = row.get(2)?;
            let pack = kind == "folder_pack";
            let members = files.entry(Some(id)).or_default();
            let mut unobserved =
                coverage.protects_history(path) || (pack && grouping.incomplete_pack(path));
            let mut has_live_file = false;
            let mut stale = Vec::new();
            let mut hashes = Vec::new();
            let mut historical_hashes = Vec::new();
            let mut analyzed = true;
            let mut cloud = false;
            for file in members {
                let path = source_relative(&root, &file.uri)?;
                if live_files.contains(path) {
                    has_live_file = true;
                    cloud |= file.cloud;
                    if let Some(hash) = file.hash.take() {
                        hashes.push((hash, file.size));
                    } else if !file.cloud {
                        analyzed = false;
                    }
                } else if coverage.protects_history(path) {
                    unobserved = true;
                } else {
                    stale.push(file.id);
                    if let Some(hash) = file.hash.take() {
                        historical_hashes.push((hash, file.size));
                    }
                }
            }
            if !coverage.incomplete && !stale.is_empty() {
                prune_files.extend_from_slice(&stale);
                resync.insert(id);
            }
            if live_models.contains(path) {
                active.insert(id);
                if !unobserved && analyzed && (stale.is_empty() || !coverage.incomplete) {
                    let hash = fingerprint(&kind, hashes);
                    tx.prepare_cached(
                        "UPDATE models SET fingerprint=?2 WHERE id=?1 AND fingerprint IS NOT ?2",
                    )?
                    .execute(params![id, hash])?;
                    if !history.contains_key(path) && !cloud {
                        if let Some(hash) = hash {
                            new.entry(hash).or_default().push(id);
                        }
                    }
                }
            } else if !unobserved {
                if let Some(previous) = history.get(path) {
                    // A path still observed on disk may simply have been regrouped.
                    if !previous.cloud
                        && !has_live_file
                        && !live_files.contains(path)
                        && !live_folders.contains(path)
                    {
                        if let Some(hash) = &previous.fingerprint {
                            // A retained fingerprint is not proof if member analysis failed
                            // or regrouping changed the old set in an earlier partial run.
                            if stale.len() == historical_hashes.len()
                                && fingerprint(&kind, historical_hashes).as_ref() == Some(hash)
                            {
                                missing.entry(hash.as_str()).or_default().push((id, stale));
                            }
                        }
                    }
                    if !coverage.incomplete && !has_live_file {
                        obsolete.insert(id);
                    }
                } else if !has_live_file && (!coverage.incomplete || stale.is_empty()) {
                    // Empty Pass 1 provisional model, not a historical deletion.
                    obsolete.insert(id);
                }
            }
        }
        drop(rows);
        drop(model_rows);
        // General pruning requires a complete walk; protected paths are never pruned.
        if !coverage.incomplete {
            for file in files.get(&None).into_iter().flatten() {
                let path = source_relative(&root, &file.uri)?;
                if !live_files.contains(path) && !coverage.protects_history(path) {
                    prune_files.push(file.id);
                }
            }
            delete_disk_files(&tx, &prune_files)?;
        }
        for (hash, destinations) in new {
            if let ([destination], Some([(original, stale)])) = (
                destinations.as_slice(),
                missing.get(hash.as_str()).map(Vec::as_slice),
            ) {
                // Only the confirmed move's old rows are removed on a partial walk.
                if coverage.incomplete {
                    delete_disk_files(&tx, stale)?;
                }
                reconcile_move(&tx, *original, *destination)?;
                obsolete.remove(original);
                active.remove(destination);
                active.insert(*original);
                resync.insert(*original);
            }
        }
        for id in obsolete {
            delete_model(&tx, id)?;
        }
        let folders = tx
            .prepare("SELECT id,rel_path FROM folders WHERE source_id=?1 ORDER BY rel_path DESC")?
            .query_map([source_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, path) in folders {
            if !coverage.protects_history(&path)
                && (grouping.folded.contains(path.as_str())
                    || (!coverage.incomplete && !live_folders.contains(path.as_str())))
            {
                tx.prepare_cached(
                    "DELETE FROM folders WHERE id=?1
                     AND NOT EXISTS (SELECT 1 FROM files WHERE folder_id=?1)
                     AND NOT EXISTS (SELECT 1 FROM models WHERE folder_id=?1)
                     AND NOT EXISTS (SELECT 1 FROM folders WHERE parent_id=?1)",
                )?
                .execute([id])?;
            }
        }
        for &id in resync.intersection(&active) {
            sync_model(&tx, &root, id)?;
        }
        let folders = refresh_folders(&tx, source_id)?;
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent {
            source_id,
            updated_folder_ids: folders,
            updated_model_ids: active.into_iter().collect(),
        })
    })
}

fn read_scan_files(
    tx: &Transaction<'_>,
    source_id: i64,
) -> rusqlite::Result<BTreeMap<Option<i64>, Vec<ScanFile>>> {
    let mut stmt = tx.prepare("SELECT id,uri,content_hash,size_bytes,storage_state='cloud_placeholder',model_id FROM files WHERE source_id=?1 AND parent_archive_id IS NULL")?;
    let mut rows = stmt.query([source_id])?;
    let mut files = BTreeMap::<_, Vec<_>>::new();
    while let Some(row) = rows.next()? {
        files.entry(row.get(5)?).or_default().push(ScanFile {
            id: row.get(0)?,
            uri: row.get(1)?,
            hash: row.get(2)?,
            size: row.get(3)?,
            cloud: row.get(4)?,
        });
    }
    Ok(files)
}

fn delete_disk_files(tx: &Transaction<'_>, ids: &[i64]) -> rusqlite::Result<()> {
    let mut fts = tx.prepare_cached("DELETE FROM files_fts WHERE rowid=?1 OR rowid IN (SELECT id FROM files WHERE parent_archive_id=?1)")?;
    let mut files = tx.prepare_cached("DELETE FROM files WHERE id=?1")?;
    for id in ids {
        fts.execute([id])?;
        files.execute([id])?;
    }
    Ok(())
}

fn delete_model(tx: &Transaction<'_>, id: i64) -> rusqlite::Result<()> {
    tx.prepare_cached("DELETE FROM models_fts WHERE rowid=?1")?
        .execute([id])?;
    tx.prepare_cached("DELETE FROM models WHERE id=?1")?
        .execute([id])?;
    Ok(())
}

fn reconcile_move(tx: &Transaction<'_>, original: i64, destination: i64) -> rusqlite::Result<()> {
    let mut indexed = tx.prepare_cached(
        "SELECT f.id,i.filename,i.rel_path,i.ext FROM files f JOIN files_fts i ON i.rowid=f.id WHERE f.model_id=?1",
    )?;
    let indexed = indexed
        .query_map([destination], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    tx.prepare_cached("UPDATE files SET model_id=?1 WHERE model_id=?2")?
        .execute(params![original, destination])?;
    for (id, name, path, ext) in indexed {
        sync_file_fts(tx, id, Some(original), &name, &path, &ext)?;
    }
    // Free the unique canonical URI before assigning it to the preserved model row.
    let mut update = tx.prepare_cached(
        "UPDATE models SET folder_id=?2, container_kind=?3, canonical_uri=?4, raw_name=?5,
          display_title=?6, primary_file_id=?7, primary_ext=?8, fingerprint=?9, updated_at=?10 WHERE id=?1",
    )?;
    let mut snapshot = tx.prepare_cached(
        "SELECT folder_id,container_kind,canonical_uri,raw_name,display_title,primary_file_id,primary_ext,fingerprint,updated_at FROM models WHERE id=?1",
    )?;
    let mut values = vec![rusqlite::types::Value::Integer(original)];
    snapshot.query_row([destination], |row| {
        for column in 0..9 {
            values.push(row.get(column)?);
        }
        Ok(())
    })?;
    delete_model(tx, destination)?;
    update.execute(rusqlite::params_from_iter(values))?;
    Ok(())
}

fn source(tx: &Transaction<'_>, source_id: i64) -> rusqlite::Result<(String, String)> {
    tx.query_row(
        "SELECT root_uri, name FROM sources WHERE id=?1",
        [source_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

fn local_uri(root: &str, path: &str) -> String {
    if path.is_empty() {
        return root.to_owned();
    }
    let separator = if root.ends_with('/') { "" } else { "/" };
    format!("{root}{separator}{path}")
}

fn local_prefix(root: &str) -> std::borrow::Cow<'_, str> {
    if root.ends_with('/') {
        root.into()
    } else {
        format!("{root}/").into()
    }
}

fn source_relative<'a>(root: &str, uri: &'a str) -> rusqlite::Result<&'a str> {
    if uri == root {
        return Ok("");
    }
    uri.strip_prefix(root)
        .and_then(|rest| {
            if root.ends_with('/') {
                Some(rest)
            } else {
                rest.strip_prefix('/')
            }
        })
        .ok_or(rusqlite::Error::InvalidQuery)
}

fn relative_to<'a>(parent: &str, path: &'a str) -> &'a str {
    if parent.is_empty() {
        path
    } else {
        &path[parent.len() + 1..]
    }
}

fn folder_id(tx: &Transaction<'_>, source_id: i64, rel_path: &str) -> rusqlite::Result<i64> {
    tx.prepare_cached("SELECT id FROM folders WHERE source_id=?1 AND rel_path=?2")?
        .query_row(params![source_id, rel_path], |r| r.get(0))
}

fn upsert_folder(
    tx: &Transaction<'_>,
    source_id: i64,
    path: &str,
    name: &str,
    visible: bool,
) -> rusqlite::Result<i64> {
    let parent = (!path.is_empty())
        .then(|| folder_id(tx, source_id, parent_path(path)))
        .transpose()?;
    if let Some(parent) = parent {
        // A folder with subdirectories cannot be absorbed as a terminal technical child.
        reveal_folder(tx, parent)?;
    }
    tx.prepare_cached(
        "INSERT INTO folders (source_id, parent_id, rel_path, name, is_visible) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(source_id, rel_path) DO UPDATE SET parent_id=excluded.parent_id, name=excluded.name,
           is_visible=max(folders.is_visible, excluded.is_visible)
         RETURNING id",
    )?.query_row(params![source_id, parent, path, name, visible], |r| r.get(0))
}

fn reveal_folder(tx: &Transaction<'_>, id: i64) -> rusqlite::Result<()> {
    tx.prepare_cached("UPDATE folders SET is_visible=1 WHERE id=?1 AND is_visible=0")?
        .execute([id])?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn upsert_model(
    tx: &Transaction<'_>,
    source: i64,
    folder: i64,
    kind: &str,
    uri: &str,
    name: &str,
    ext: &str,
    cloud: bool,
) -> rusqlite::Result<i64> {
    tx.prepare_cached(
        "INSERT INTO models (source_id, folder_id, container_kind, canonical_uri, raw_name, display_title,
           primary_ext, has_cloud_placeholder, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(canonical_uri) DO UPDATE SET folder_id=excluded.folder_id, container_kind=excluded.container_kind,
           raw_name=excluded.raw_name, display_title=excluded.display_title, primary_ext=excluded.primary_ext,
           has_cloud_placeholder=excluded.has_cloud_placeholder, updated_at=excluded.updated_at
         RETURNING id",
    )?.query_row(params![source, folder, kind, uri, name, display_title(name), ext, cloud, unix_seconds(SystemTime::now())], |r| r.get(0))
}

fn sync_file_fts(
    tx: &Transaction<'_>,
    id: i64,
    model: Option<i64>,
    name: &str,
    path: &str,
    ext: &str,
) -> rusqlite::Result<()> {
    tx.prepare_cached(
        "INSERT OR REPLACE INTO files_fts (rowid, file_id, model_id, filename, rel_path, ext)
         SELECT ?1,?1,?2,?3,?4,?5 WHERE NOT EXISTS (
           SELECT 1 FROM files_fts WHERE rowid=?1 AND file_id IS ?1 AND model_id IS ?2
             AND filename IS ?3 AND rel_path IS ?4 AND ext IS ?5)",
    )?
    .execute(params![id, model, name, path, ext])?;
    Ok(())
}

fn sync_model(tx: &Transaction<'_>, root: &str, model: i64) -> rusqlite::Result<()> {
    tx.prepare_cached(
        "WITH totals(n, bytes, cloud) AS (
            SELECT count(*), coalesce(sum(size_bytes),0), coalesce(max(storage_state='cloud_placeholder'),0)
            FROM files WHERE model_id=?1
         ) UPDATE models SET file_count=n,total_bytes=bytes,has_cloud_placeholder=cloud FROM totals
         WHERE id=?1 AND (file_count,total_bytes,has_cloud_placeholder) IS NOT (n,bytes,cloud)",
    )?.execute([model])?;
    tx.prepare_cached(
        "WITH desired(id, title, path_tokens, file_tokens) AS MATERIALIZED (
           SELECT id, display_title || ' ' || raw_name, substr(canonical_uri,length(?2)+1),
             (SELECT group_concat(rel_path || ' ' || ext, ' ' ORDER BY rel_path, id) FROM files WHERE model_id=?1)
           FROM models WHERE id=?1
         )
         INSERT OR REPLACE INTO models_fts (rowid, model_id, title, path_tokens, file_tokens)
         SELECT id,id,title,path_tokens,file_tokens FROM desired d WHERE NOT EXISTS (
           SELECT 1 FROM models_fts i WHERE i.rowid=d.id AND i.model_id IS d.id AND i.title IS d.title
             AND i.path_tokens IS d.path_tokens AND i.file_tokens IS d.file_tokens)",
    )?.execute(params![model, root])?;
    Ok(())
}

#[derive(Default)]
struct Aggregate {
    models: i64,
    files: i64,
    folder_count: i64,
    titles: Vec<String>,
    formats: BTreeMap<String, i64>,
    cloud: i64,
}

fn refresh_folders(tx: &Transaction<'_>, source: i64) -> Result<Vec<i64>, IpcError> {
    let folders = tx.prepare("SELECT id, parent_id, is_visible FROM folders WHERE source_id=?1 ORDER BY rel_path COLLATE BINARY DESC")?
        .query_map([source], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, bool>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    let mut direct = BTreeMap::<i64, Aggregate>::new();
    let mut stmt = tx.prepare("SELECT folder_id, display_title FROM models WHERE source_id=?1 AND folder_id IS NOT NULL ORDER BY folder_id, display_title COLLATE NOCASE, id")?;
    let mut rows = stmt.query([source])?;
    while let Some(row) = rows.next()? {
        let folder = direct.entry(row.get(0)?).or_default();
        folder.models += 1;
        if folder.titles.len() < 4 {
            folder.titles.push(row.get(1)?);
        }
    }
    let mut stmt = tx.prepare("SELECT folder_id, ext, count(*), sum(storage_state='cloud_placeholder') FROM files WHERE source_id=?1 AND folder_id IS NOT NULL GROUP BY folder_id, ext")?;
    let mut rows = stmt.query([source])?;
    while let Some(row) = rows.next()? {
        let folder = direct.entry(row.get(0)?).or_default();
        let count = row.get::<_, i64>(2)?;
        folder.files += count;
        folder.formats.insert(row.get(1)?, count);
        folder.cloud += row.get::<_, i64>(3)?;
    }
    let mut recursive = BTreeMap::<i64, Aggregate>::new();
    let mut update = tx.prepare("UPDATE folders SET model_count=?2,file_count=?3,recursive_model_count=?4,
        recursive_file_count=?5,nested_folder_count=?6,sample_titles_json=?7,summary_json=?8 WHERE id=?1
        AND (model_count,file_count,recursive_model_count,recursive_file_count,nested_folder_count,sample_titles_json,summary_json)
        IS NOT (?2,?3,?4,?5,?6,?7,?8)")?;
    for &(id, parent, visible) in &folders {
        let here = direct.remove(&id).unwrap_or_default();
        let mut tree = recursive.remove(&id).unwrap_or_default();
        let (model_count, file_count) = (here.models, here.files);
        tree.merge(here);
        let mut summary = json!(tree.formats);
        summary["cloud"] = json!(tree.cloud);
        update.execute(params![
            id,
            model_count,
            file_count,
            tree.models,
            tree.files,
            tree.folder_count,
            json!(tree.titles).to_string(),
            summary.to_string()
        ])?;
        if let Some(parent) = parent {
            tree.folder_count += i64::from(visible);
            recursive.entry(parent).or_default().merge(tree);
        }
    }
    Ok(folders.into_iter().map(|(id, _, _)| id).collect())
}

impl Aggregate {
    fn merge(&mut self, other: Self) {
        self.models += other.models;
        self.files += other.files;
        self.folder_count += other.folder_count;
        self.cloud += other.cloud;
        self.titles.extend(other.titles);
        self.titles.sort_unstable();
        self.titles.truncate(4);
        for (ext, count) in other.formats {
            *self.formats.entry(ext).or_default() += count;
        }
    }
}
