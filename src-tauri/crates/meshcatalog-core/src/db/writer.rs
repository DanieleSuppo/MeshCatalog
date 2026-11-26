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
        grouping::{is_image, parent_path, Grouping},
        title::display_title,
        walker::InventoryEntry,
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
                InventoryEntry::Directory { rel_path, name, .. } => {
                    let id = upsert_folder(&tx, source_id, rel_path, if rel_path.is_empty() { &source_name } else { name })?;
                    folders.insert(id);
                }
                InventoryEntry::OtherFile { .. } => {}
                InventoryEntry::File { rel_path, name, ext, metadata } => {
                    let folder = folder_id(&tx, source_id, parent_path(rel_path))?;
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
        // Keep historical models available for move matching even if regrouping emptied them.
        let empty = "SELECT id FROM models WHERE source_id=?1 AND fingerprint IS NULL AND NOT EXISTS (SELECT 1 FROM files WHERE model_id=models.id)";
        tx.execute(&format!("DELETE FROM models_fts WHERE rowid IN ({empty})"), [source_id])?;
        tx.execute(&format!("DELETE FROM models WHERE id IN ({empty})"), [source_id])?;
        for path in grouping.folded.iter().rev() {
            tx.prepare_cached(
                "DELETE FROM folders WHERE source_id=?1 AND rel_path=?2
                   AND NOT EXISTS (SELECT 1 FROM files WHERE folder_id=folders.id)
                   AND NOT EXISTS (SELECT 1 FROM models WHERE folder_id=folders.id)
                   AND NOT EXISTS (SELECT 1 FROM folders child WHERE child.source_id=?1 AND child.parent_id=folders.id)",
            )?.execute(params![source_id, path])?;
        }
        for &id in &model_ids {
            sync_model(&tx, &root, id)?;
        }
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

pub(crate) type ModelHistory = BTreeMap<String, (i64, Option<String>)>;

pub(crate) fn scan_history(db: &Database, source_id: i64) -> Result<ModelHistory, IpcError> {
    db.with_reader(|conn| {
        let mut stmt =
            conn.prepare("SELECT canonical_uri,id,fingerprint FROM models WHERE source_id=?1")?;
        let rows = stmt.query_map([source_id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
        Ok(rows.collect::<Result<_, _>>()?)
    })
}

pub(crate) fn finish_scan(
    db: &Database,
    source_id: i64,
    entries: &[InventoryEntry],
    grouping: &Grouping<'_>,
    history: &ModelHistory,
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let (root, _) = source(&tx, source_id)?;
        let mut live_files = BTreeSet::new();
        let mut live_folders = BTreeSet::new();
        for entry in entries {
            match entry {
                InventoryEntry::File { rel_path, .. } => { live_files.insert(local_uri(&root, rel_path)); }
                InventoryEntry::Directory { rel_path, .. } if !grouping.folded.contains(rel_path.as_str()) => {
                    live_folders.insert(rel_path.as_str());
                }
                _ => {}
            }
        }
        let mut changed_models = prune_files(&tx, source_id, &live_files)?;
        let live_models: BTreeSet<_> = grouping.models.iter().map(|m| local_uri(&root, m.path)).collect();
        let mut missing = BTreeMap::<_, Vec<_>>::new();
        for (uri, (id, hash)) in history {
            if !live_models.contains(uri) {
                if let Some(hash) = hash { missing.entry(hash.as_str()).or_default().push(*id); }
            }
        }
        let mut new = BTreeMap::<String, Vec<i64>>::new();
        let mut active = BTreeSet::new();
        let mut model_rows = tx.prepare("SELECT id,canonical_uri,container_kind FROM models WHERE source_id=?1")?;
        let mut rows = model_rows.query([source_id])?;
        let mut obsolete = BTreeSet::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let uri: String = row.get(1)?;
            if !live_models.contains(&uri) {
                obsolete.insert(id);
                continue;
            }
            let kind: String = row.get(2)?;
            let mut members = tx.prepare_cached(
                "SELECT content_hash,size_bytes FROM files WHERE model_id=?1 AND storage_state='local_resident' AND content_hash IS NOT NULL",
            )?;
            let members = members.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            let hash = fingerprint(&kind, members);
            tx.prepare_cached("UPDATE models SET fingerprint=?2 WHERE id=?1 AND fingerprint IS NOT ?2")?.execute(params![id, hash])?;
            if !history.contains_key(&uri) {
                if let Some(hash) = hash { new.entry(hash).or_default().push(id); }
            }
            active.insert(id);
        }
        drop(rows);
        for (hash, destinations) in new {
            if let ([destination], Some([original])) = (destinations.as_slice(), missing.get(hash.as_str()).map(Vec::as_slice)) {
                reconcile_move(&tx, *original, *destination)?;
                obsolete.remove(original);
                active.remove(destination);
                active.insert(*original);
                changed_models.insert(*original);
            }
        }
        for id in obsolete {
            // Surviving files were all reassigned by grouping before reaching this transaction.
            delete_model(&tx, id)?;
        }
        let folders = tx.prepare("SELECT id,rel_path FROM folders WHERE source_id=?1 ORDER BY rel_path DESC")?
            .query_map([source_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        for (id, path) in folders {
            if !live_folders.contains(path.as_str()) {
                tx.prepare_cached("DELETE FROM folders WHERE id=?1")?.execute([id])?;
            }
        }
        for &id in changed_models.intersection(&active) { sync_model(&tx, &root, id)?; }
        let folders = refresh_folders(&tx, source_id)?;
        drop(model_rows);
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent { source_id, updated_folder_ids: folders, updated_model_ids: active.into_iter().collect() })
    })
}

fn prune_files(
    tx: &Transaction<'_>,
    source_id: i64,
    live: &BTreeSet<String>,
) -> rusqlite::Result<BTreeSet<i64>> {
    let mut files = tx.prepare(
        "SELECT id,uri,ext,model_id FROM files WHERE source_id=?1 AND parent_archive_id IS NULL",
    )?;
    let stale = files
        .query_map([source_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut affected = BTreeSet::new();
    for (id, uri, ext, model) in stale {
        if !live.contains(&uri) {
            if let Some(model) = model {
                affected.insert(model);
            }
            if ext == "zip" {
                tx.prepare_cached("DELETE FROM files_fts WHERE rowid IN (SELECT id FROM files WHERE parent_archive_id=?1)")?.execute([id])?;
            }
            tx.prepare_cached("DELETE FROM files_fts WHERE rowid=?1")?
                .execute([id])?;
            tx.prepare_cached("DELETE FROM files WHERE id=?1")?
                .execute([id])?;
        }
    }
    Ok(affected)
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

pub(crate) fn write_folder_aggregates(
    db: &Database,
    source_id: i64,
) -> Result<IndexerBatchUpdatedEvent, IpcError> {
    db.with_writer(|conn| {
        let tx = conn.transaction()?;
        let folders = refresh_folders(&tx, source_id)?;
        tx.commit()?;
        Ok(IndexerBatchUpdatedEvent {
            source_id,
            updated_folder_ids: folders,
            updated_model_ids: Vec::new(),
        })
    })
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
) -> rusqlite::Result<i64> {
    let parent = (!path.is_empty())
        .then(|| folder_id(tx, source_id, parent_path(path)))
        .transpose()?;
    tx.prepare_cached(
        "INSERT INTO folders (source_id, parent_id, rel_path, name) VALUES (?1,?2,?3,?4)
         ON CONFLICT(source_id, rel_path) DO UPDATE SET parent_id=excluded.parent_id, name=excluded.name
         RETURNING id",
    )?.query_row(params![source_id, parent, path, name], |r| r.get(0))
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
    tx.prepare_cached("DELETE FROM files_fts WHERE rowid=?1")?
        .execute([id])?;
    tx.prepare_cached("INSERT INTO files_fts (rowid, file_id, model_id, filename, rel_path, ext) VALUES (?1,?1,?2,?3,?4,?5)")?
        .execute(params![id, model, name, path, ext])?;
    Ok(())
}

fn sync_model(tx: &Transaction<'_>, root: &str, model: i64) -> rusqlite::Result<()> {
    tx.prepare_cached(
        "UPDATE models SET (file_count, total_bytes, has_cloud_placeholder, fingerprint) = (
            SELECT count(*), coalesce(sum(size_bytes),0), coalesce(max(storage_state='cloud_placeholder'),0),
              CASE WHEN max(storage_state='local_resident') THEN models.fingerprint END
            FROM files WHERE model_id=?1) WHERE id=?1",
    )?.execute([model])?;
    tx.prepare_cached("DELETE FROM models_fts WHERE rowid=?1")?
        .execute([model])?;
    tx.prepare_cached(
        "INSERT INTO models_fts (rowid, model_id, title, path_tokens, file_tokens)
         SELECT id,id,display_title || ' ' || raw_name, substr(canonical_uri,length(?2)+1),
           (SELECT group_concat(rel_path || ' ' || ext, ' ' ORDER BY rel_path, id) FROM files WHERE model_id=?1)
         FROM models WHERE id=?1",
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
    let folders = tx.prepare("SELECT id, parent_id FROM folders WHERE source_id=?1 ORDER BY rel_path COLLATE BINARY DESC")?
        .query_map([source], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?)))?.collect::<Result<Vec<_>, _>>()?;
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
        recursive_file_count=?5,nested_folder_count=?6,sample_titles_json=?7,summary_json=?8 WHERE id=?1")?;
    for &(id, parent) in &folders {
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
            tree.folder_count += 1;
            recursive.entry(parent).or_default().merge(tree);
        }
    }
    Ok(folders.into_iter().map(|(id, _)| id).collect())
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
