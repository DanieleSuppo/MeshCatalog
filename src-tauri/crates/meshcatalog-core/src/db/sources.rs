use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use rusqlite::{params, OptionalExtension};

use crate::{
    contracts::{SourceDto, SourceKind, SourceStatus},
    error::IpcError,
    scanner::{
        attr_guard::{unix_seconds, OsMetadataInspector},
        walker::check_source_directory,
    },
};

use super::Database;

impl Database {
    pub fn list_sources(&self) -> Result<Vec<SourceDto>, IpcError> {
        self.with_reader(|conn| {
            let mut statement = conn.prepare(
                "SELECT s.id, s.kind, s.name, s.root_uri, s.status, s.last_error, s.last_scan_at,
                        (SELECT count(*) FROM models WHERE source_id = s.id),
                        (SELECT count(*) FROM files WHERE source_id = s.id)
                 FROM sources s ORDER BY s.id",
            )?;
            let rows = statement.query_map([], |row| {
                let kind = match row.get_ref(1)?.as_str()? {
                    "local_dir" => SourceKind::LocalDir,
                    "dropbox" => SourceKind::Dropbox,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                let status = match row.get_ref(4)?.as_str()? {
                    "idle" => SourceStatus::Idle,
                    "scanning" => SourceStatus::Scanning,
                    "unavailable" => SourceStatus::Unavailable,
                    "error" => SourceStatus::Error,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                let root_uri: String = row.get(3)?;
                Ok(SourceDto {
                    id: row.get(0)?,
                    kind,
                    name: row.get(2)?,
                    root_display_path: root_uri
                        .strip_prefix("local://")
                        .unwrap_or(&root_uri)
                        .to_owned(),
                    root_uri,
                    status,
                    last_error: row.get(5)?,
                    last_scan_at: row.get(6)?,
                    model_count: row.get(7)?,
                    file_count: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
    }

    pub fn add_local_source(&self, path: &Path, name: Option<&str>) -> Result<i64, IpcError> {
        if path.as_os_str().is_empty() {
            return Err(IpcError::InvalidPath);
        }
        let absolute = std::path::absolute(path).map_err(|_| IpcError::InvalidPath)?;
        check_source_directory(&absolute, &OsMetadataInspector)
            .map_err(|_| IpcError::InvalidPath)?;
        let canonical = fs::canonicalize(&absolute).map_err(|_| IpcError::InvalidPath)?;
        check_source_directory(&canonical, &OsMetadataInspector)
            .map_err(|_| IpcError::InvalidPath)?;
        fs::read_dir(&canonical).map_err(|_| IpcError::InvalidPath)?;
        let display = display_path(&canonical)?;
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&display)
            });
        let root_uri = format!("local://{display}");
        let candidate = overlap_key(&display);
        self.with_writer(|conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            {
                let mut statement = tx.prepare("SELECT root_uri FROM sources WHERE kind = 'local_dir'")?;
                let roots = statement.query_map([], |row| row.get::<_, String>(0))?;
                for root in roots {
                    let root = root?;
                    let existing = overlap_key(root.strip_prefix("local://").unwrap_or(&root));
                    if candidate.starts_with(&existing) || existing.starts_with(&candidate) {
                        return Err(IpcError::SourceOverlap);
                    }
                }
            }
            let id = tx.query_row(
                "INSERT INTO sources (kind, name, root_uri, created_at) VALUES ('local_dir', ?1, ?2, ?3) RETURNING id",
                params![name, root_uri, unix_seconds(SystemTime::now())],
                |r| r.get(0),
            )?;
            tx.commit()?;
            Ok(id)
        })
    }

    pub fn remove_source(&self, id: i64) -> Result<(), IpcError> {
        self.with_writer(|conn| {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM files_fts WHERE file_id IN (SELECT id FROM files WHERE source_id = ?1)", [id])?;
            tx.execute("DELETE FROM models_fts WHERE model_id IN (SELECT id FROM models WHERE source_id = ?1)", [id])?;
            if tx.execute("DELETE FROM sources WHERE id = ?1", [id])? == 0 {
                return Err(IpcError::SourceNotFound);
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub(crate) fn local_source_root(&self, id: i64) -> Result<PathBuf, IpcError> {
        let uri: String = self.with_reader(|conn| {
            conn.query_row(
                "SELECT root_uri FROM sources WHERE kind = 'local_dir' AND id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(IpcError::SourceNotFound)
        })?;
        let path = uri.strip_prefix("local://").ok_or(IpcError::InvalidPath)?;
        #[cfg(windows)]
        let path = {
            // Restore the canonical verbatim form, including long paths and UNC roots.
            let native = path.replace('/', "\\");
            if let Some(unc) = native.strip_prefix(r"\\") {
                format!(r"\\?\UNC\{unc}")
            } else {
                format!(r"\\?\{native}")
            }
        };
        Ok(PathBuf::from(path))
    }

    pub fn mark_source_status(
        &self,
        id: i64,
        status: SourceStatus,
        last_error: Option<&str>,
        completed: bool,
    ) -> Result<(), IpcError> {
        let status = match status {
            SourceStatus::Idle => "idle",
            SourceStatus::Scanning => "scanning",
            SourceStatus::Unavailable => "unavailable",
            SourceStatus::Error => "error",
        };
        self.with_writer(|conn| {
            let now = completed.then(|| unix_seconds(SystemTime::now()));
            if conn.execute(
                "UPDATE sources SET status = ?2, last_error = ?3, last_scan_at = coalesce(?4, last_scan_at) WHERE id = ?1",
                params![id, status, last_error, now],
            )? == 0 {
                return Err(IpcError::SourceNotFound);
            }
            Ok(())
        })
    }
}

fn display_path(canonical: &Path) -> Result<String, IpcError> {
    let path = canonical.to_str().ok_or(IpcError::InvalidPath)?;
    #[cfg(windows)]
    let path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!("//{}", unc.replace('\\', "/"))
    } else {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('\\', "/")
    };
    Ok(path.to_owned())
}

fn overlap_key(display: &str) -> PathBuf {
    #[cfg(windows)]
    let display = display.to_uppercase();
    PathBuf::from(display)
}
