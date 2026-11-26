use std::{collections::BTreeMap, sync::LazyLock, time::Instant};

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::{
    contracts::{
        ContainerKind, FileCardDto, FolderCardDto, FolderTreeNode, ModelCardDto, ModelDetailDto,
        SearchItemDto, SearchQuery, SearchResultPage, SourceFilter, StorageState, ViewMode,
    },
    error::IpcError,
};

use super::Database;

const SOURCE_SCOPE: &str = "(?2 IS NULL OR i.source_id=?2)
    AND (?3 IS NULL OR i.source_id IN (SELECT id FROM sources WHERE kind=?3))";
const SUBTREE: &str = "WITH RECURSIVE subtree(id,source_id) AS (
    SELECT id,source_id FROM folders WHERE id=?1
    UNION ALL SELECT f.id,f.source_id FROM subtree p JOIN folders f ON f.source_id=p.source_id AND f.parent_id=p.id
) ";
const MODEL_COLUMNS: &str = "i.id,i.source_id,i.folder_id,i.container_kind,i.canonical_uri,i.display_title,
    i.raw_name,i.primary_ext,i.has_cloud_placeholder,i.file_count,i.total_bytes,i.updated_at,s.root_uri";
const FILE_COLUMNS: &str =
    "i.id,i.model_id,i.source_id,i.folder_id,i.parent_archive_id,p.filename,i.uri,
    i.rel_path,i.filename,i.ext,i.storage_state,i.size_bytes,i.mtime_unix,s.root_uri,p.uri";

impl Database {
    pub fn get_folder_tree(&self, filter: SourceFilter) -> Result<Vec<FolderTreeNode>, IpcError> {
        self.with_reader(|conn| {
            let mut statement = conn.prepare_cached(
                "SELECT f.id,f.source_id,f.parent_id,f.rel_path,f.name,
                f.recursive_model_count,f.recursive_file_count,f.nested_folder_count
                FROM folders f JOIN sources s ON s.id=f.source_id WHERE (?1 IS NULL OR s.kind=?1)
                ORDER BY f.name COLLATE NOCASE,f.id",
            )?;
            let rows = statement.query_map([source_kind(filter)], |row| {
                Ok(FolderTreeNode {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    rel_path: row.get(3)?,
                    name: row.get(4)?,
                    recursive_model_count: row.get(5)?,
                    recursive_file_count: row.get(6)?,
                    nested_folder_count: row.get(7)?,
                    children: Vec::new(),
                })
            })?;
            let mut children: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for row in rows {
                let node = row?;
                children.entry(node.parent_id).or_default().push(node);
            }
            Ok(build_tree(&mut children, None))
        })
    }

    pub fn search_catalog(&self, query: &SearchQuery) -> Result<SearchResultPage, IpcError> {
        let start = Instant::now();
        let browse = query.text.trim().is_empty();
        let fts = (!browse).then(|| literal_fts(&query.text));
        let mut page = SearchResultPage {
            total_models: 0,
            total_files: 0,
            subfolders: Vec::new(),
            items: Vec::new(),
            facet_counts: BTreeMap::new(),
            query_time_ms: 0.0,
        };
        if !fts.as_ref().is_some_and(String::is_empty) {
            let scope = Scope {
                query,
                fts: fts.as_deref(),
            };
            self.with_reader(|conn| {
                // Counts and page use one WAL snapshot, including when an indexing batch commits.
                let tx = conn.unchecked_transaction()?;
                page.total_models = scope.count(&tx, ViewMode::Models)?;
                page.total_files = scope.count(&tx, ViewMode::Files)?;
                if browse {
                    page.subfolders = subfolders(&tx, query)?;
                }
                if !browse || query.folder_id.is_some() {
                    page.items = scope.items(&tx)?;
                }
                tx.commit()?;
                Ok(())
            })?;
        }
        page.query_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        Ok(page)
    }

    pub fn get_model_detail(&self, model_id: i64) -> Result<ModelDetailDto, IpcError> {
        self.with_reader(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut detail = tx.query_row(
                "SELECT i.id,i.source_id,s.name,i.container_kind,i.canonical_uri,i.display_title,i.raw_name,
                  i.primary_ext,i.has_cloud_placeholder,i.file_count,i.total_bytes,i.updated_at
                 FROM models i JOIN sources s ON s.id=i.source_id WHERE i.id=?1", [model_id], |row| Ok(ModelDetailDto {
                    id: row.get(0)?, source_id: row.get(1)?, source_name: row.get(2)?, container_kind: container(row, 3)?,
                    canonical_uri: row.get(4)?, display_title: row.get(5)?, raw_name: row.get(6)?,
                    primary_ext: row.get(7)?, has_cloud_placeholder: row.get(8)?, file_count: row.get(9)?,
                    total_bytes: row.get(10)?, updated_at: row.get(11)?, files: Vec::new(),
                 }),
            ).optional()?.ok_or(IpcError::ModelNotFound)?;
            detail.files = tx.prepare_cached(&format!("SELECT {FILE_COLUMNS} FROM files i
                    JOIN sources s ON s.id=i.source_id LEFT JOIN files p ON p.id=i.parent_archive_id
                    WHERE i.model_id=?1 ORDER BY i.rel_path COLLATE NOCASE,i.id"))?
                .query_map([model_id], |row| file_card(row, false))?.collect::<Result<_, _>>()?;
            tx.commit()?;
            Ok(detail)
        })
    }
}

struct Scope<'a> {
    query: &'a SearchQuery,
    fts: Option<&'a str>,
}

impl Scope<'_> {
    fn parts(&self, view: ViewMode) -> (&'static str, &'static str, &'static str) {
        let recursive = self.fts.is_some() && self.query.folder_id.is_some();
        let cte = if recursive { SUBTREE } else { "" };
        let folder = match (recursive, self.query.folder_id) {
            (true, _) => "i.folder_id IN (SELECT id FROM subtree)",
            (false, Some(_)) => "i.folder_id=?1",
            (false, None) => "?1 IS NULL",
        };
        let table = match (self.fts.is_some(), view) {
            (true, ViewMode::Models) => "models_fts CROSS JOIN models i ON i.id=models_fts.rowid",
            (true, ViewMode::Files) => "files_fts CROSS JOIN files i ON i.id=files_fts.rowid",
            (false, ViewMode::Models) => "models i",
            (false, ViewMode::Files) => "files i",
        };
        (cte, table, folder)
    }

    fn fts_predicate(&self, view: ViewMode) -> &'static str {
        match (self.fts.is_some(), view) {
            (true, ViewMode::Models) => "models_fts MATCH ?4",
            (true, ViewMode::Files) => "files_fts MATCH ?4",
            (false, _) => "?4 IS NULL",
        }
    }

    fn count(&self, conn: &Connection, view: ViewMode) -> rusqlite::Result<i64> {
        let (cte, table, folder) = self.parts(view);
        let fts = self.fts_predicate(view);
        conn.prepare_cached(&format!(
            "{cte}SELECT count(*) FROM {table} WHERE {fts} AND {folder} AND {SOURCE_SCOPE}"
        ))?
        .query_row(
            params![
                self.query.folder_id,
                self.query.source_id,
                source_kind(self.query.source_filter),
                self.fts
            ],
            |row| row.get(0),
        )
    }

    fn items(&self, conn: &Connection) -> rusqlite::Result<Vec<SearchItemDto>> {
        let view = self.query.view_mode;
        let (cte, table, folder) = self.parts(view);
        let fts = self.fts_predicate(view);
        let (columns, parent_join, order) = match view {
            ViewMode::Models => (MODEL_COLUMNS, "", "i.display_title COLLATE NOCASE,i.id"),
            ViewMode::Files => (
                FILE_COLUMNS,
                "LEFT JOIN files p ON p.id=i.parent_archive_id",
                "i.filename COLLATE NOCASE,i.id",
            ),
        };
        conn.prepare_cached(&format!(
            "{cte}SELECT {columns} FROM {table} JOIN sources s ON s.id=i.source_id {parent_join}
             WHERE {fts} AND {folder} AND {SOURCE_SCOPE} ORDER BY {order} LIMIT ?5 OFFSET ?6"
        ))?
        .query_map(
            params![
                self.query.folder_id,
                self.query.source_id,
                source_kind(self.query.source_filter),
                self.fts,
                self.query.limit,
                self.query.offset
            ],
            |row| match view {
                ViewMode::Models => model_card(row).map(SearchItemDto::Model),
                ViewMode::Files => file_card(row, true).map(SearchItemDto::File),
            },
        )?
        .collect()
    }
}

fn subfolders(conn: &Connection, query: &SearchQuery) -> rusqlite::Result<Vec<FolderCardDto>> {
    conn.prepare_cached(&format!("SELECT i.id,i.source_id,i.rel_path,i.name,i.recursive_model_count,i.recursive_file_count,
        i.nested_folder_count,coalesce(i.sample_titles_json,'[]'),coalesce(json_extract(i.summary_json,'$.cloud'),0)>0
        FROM folders i WHERE i.parent_id IS ?1 AND {SOURCE_SCOPE} ORDER BY i.name COLLATE NOCASE,i.id"))?
        .query_map(params![query.folder_id,query.source_id,source_kind(query.source_filter)], |r| {
            let titles = r.get_ref(7)?.as_str()?;
            let sample_titles = serde_json::from_str(titles).map_err(|error| rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error)))?;
            Ok(FolderCardDto { id: r.get(0)?, source_id: r.get(1)?, rel_path: r.get(2)?, name: r.get(3)?,
                recursive_model_count: r.get(4)?, recursive_file_count: r.get(5)?, nested_folder_count: r.get(6)?,
                sample_titles, has_cloud_placeholder: r.get(8)? })
        })?.collect()
}

fn literal_fts(text: &str) -> String {
    static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}\p{M}]+").unwrap());
    TOKEN
        .find_iter(text)
        .map(|token| format!("\"{}\"", token.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_tree(
    children: &mut BTreeMap<Option<i64>, Vec<FolderTreeNode>>,
    parent: Option<i64>,
) -> Vec<FolderTreeNode> {
    let mut nodes = children.remove(&parent).unwrap_or_default();
    for node in &mut nodes {
        node.children = build_tree(children, Some(node.id));
    }
    nodes
}

fn source_kind(filter: SourceFilter) -> Option<&'static str> {
    match filter {
        SourceFilter::All => None,
        SourceFilter::Local => Some("local_dir"),
        SourceFilter::Dropbox => Some("dropbox"),
    }
}

fn relative_to_root<'a>(uri: &'a str, root: &str) -> &'a str {
    uri.strip_prefix(root)
        .map(|path| path.trim_start_matches('/'))
        .unwrap_or(uri)
}

fn model_card(row: &Row<'_>) -> rusqlite::Result<ModelCardDto> {
    let uri: String = row.get(4)?;
    let relative_path = relative_to_root(&uri, row.get_ref(12)?.as_str()?).to_owned();
    Ok(ModelCardDto {
        id: row.get(0)?,
        source_id: row.get(1)?,
        folder_id: row.get(2)?,
        container_kind: container(row, 3)?,
        canonical_uri: uri,
        display_title: row.get(5)?,
        raw_name: row.get(6)?,
        primary_ext: row.get(7)?,
        has_cloud_placeholder: row.get(8)?,
        file_count: row.get(9)?,
        total_bytes: row.get(10)?,
        updated_at: row.get(11)?,
        relative_path,
    })
}

fn file_card(row: &Row<'_>, contextual: bool) -> rusqlite::Result<FileCardDto> {
    let uri: String = row.get(6)?;
    let internal_path: String = row.get(7)?;
    let rel_path = if contextual {
        let root = row.get_ref(13)?.as_str()?;
        match row.get::<_, Option<String>>(14)? {
            Some(parent) => format!("{}/{internal_path}", relative_to_root(&parent, root)),
            None => relative_to_root(&uri, root).to_owned(),
        }
    } else {
        internal_path
    };
    let storage_state = match row.get_ref(10)?.as_str()? {
        "local_resident" => StorageState::LocalResident,
        "cloud_placeholder" => StorageState::CloudPlaceholder,
        "dropbox_remote" => StorageState::DropboxRemote,
        "zip_entry" => StorageState::ZipEntry,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(FileCardDto {
        id: row.get(0)?,
        model_id: row.get(1)?,
        source_id: row.get(2)?,
        folder_id: row.get(3)?,
        parent_archive_id: row.get(4)?,
        parent_archive_name: row.get(5)?,
        uri,
        rel_path,
        filename: row.get(8)?,
        ext: row.get(9)?,
        storage_state,
        size_bytes: row.get(11)?,
        mtime_unix: row.get(12)?,
    })
}

fn container(row: &Row<'_>, column: usize) -> rusqlite::Result<ContainerKind> {
    match row.get_ref(column)?.as_str()? {
        "folder_pack" => Ok(ContainerKind::FolderPack),
        "zip_archive" => Ok(ContainerKind::ZipArchive),
        "single_file" => Ok(ContainerKind::SingleFile),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
