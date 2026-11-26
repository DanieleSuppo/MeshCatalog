//! Background indexing passes.

use std::{collections::BTreeSet, fs, io::Seek, path::Path, thread};

use rayon::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::{
    archive::zip_reader::read_zip_directory,
    contracts::{
        IndexerBatchUpdatedEvent, IndexerErrorEvent, IndexerPass, IndexerProgressEvent,
        SourceStatus,
    },
    db::{
        writer::{
            cached_analysis, finish_scan, legacy_root_children, scan_history, write_analysis,
            write_groups, write_inventory_batch, FileAnalysis, ModelHistory,
        },
        Database,
    },
    error::IpcError,
    hasher::content_hash,
    scanner::{
        attr_guard::{open_resident, EntryMetadata, EntryMetadataInspector},
        grouping::{group, Grouping},
        walker::{check_source_directory, walk_inventory, InventoryEntry, WalkCoverage},
    },
};

const BATCH_SIZE: usize = 500;
pub const INVENTORY_LABEL: &str = "Passata 1/2 — Inventario cartelle";
const METADATA_LABEL: &str = "Passata 2/2 — Analisi ZIP e impronte";

pub trait PipelineSink {
    fn progress(&mut self, event: IndexerProgressEvent);
    fn batch_updated(&mut self, event: IndexerBatchUpdatedEvent);
    fn error(&mut self, event: IndexerErrorEvent);
}

#[derive(Debug, Default)]
pub struct Pass1Stats {
    pub models_indexed: u64,
    pub files_indexed: u64,
    pub errors_count: u64,
    pub cancelled: bool,
}

pub fn run_pass1(
    db: &Database,
    source_id: i64,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
    sink: &mut impl PipelineSink,
) -> Result<Pass1Stats, IpcError> {
    run(db, source_id, inspector, cancel, sink, |_, _, _| {
        Ok(FinishOutcome::default())
    })
}

pub fn run_scan(
    db: &Database,
    source_id: i64,
    inspector: &(impl EntryMetadataInspector + Sync),
    cancel: &CancellationToken,
    sink: &mut impl PipelineSink,
) -> Result<Pass1Stats, IpcError> {
    run(
        db,
        source_id,
        inspector,
        cancel,
        sink,
        |inventory, root, grouped| inventory.finish(root, inspector, cancel, grouped),
    )
}

fn run<S: PipelineSink>(
    db: &Database,
    source_id: i64,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
    sink: &mut S,
    finish: impl FnOnce(
        &mut Inventory<'_, S>,
        &Path,
        GroupedInventory<'_>,
    ) -> Result<FinishOutcome, IpcError>,
) -> Result<Pass1Stats, IpcError> {
    let root = db.local_source_root(source_id)?;
    if cancel.is_cancelled() {
        return Ok(Pass1Stats {
            cancelled: true,
            ..Pass1Stats::default()
        });
    }
    let history = scan_history(db, source_id)?;
    db.mark_source_status(source_id, SourceStatus::Scanning, None, false)?;
    let mut inventory = Inventory {
        db,
        source_id,
        sink,
        entries: Vec::with_capacity(BATCH_SIZE),
        recorded: Vec::new(),
        stats: Pass1Stats::default(),
        current_item: String::new(),
    };
    inventory.progress(0);
    let mut unavailable = false;
    let mut coverage = WalkCoverage::default();
    let mut opaque = false;
    let mut walk_failed = false;
    let mut walk_notices = Notices::default();
    let mut metadata_notices = Notices::default();
    let result = walk_inventory(&root, inspector, cancel, |entry| {
        match entry {
            Ok(entry) => {
                if let InventoryEntry::File { rel_path, .. } = &entry {
                    inventory.current_item.clone_from(rel_path);
                }
                match &entry {
                    InventoryEntry::Directory {
                        rel_path,
                        opaque: true,
                        ..
                    } => {
                        coverage.exclude(rel_path);
                        coverage.incomplete = true;
                        opaque = true;
                    }
                    InventoryEntry::UnobservedLink { rel_path } => {
                        coverage.exclude_link(rel_path);
                        return Ok(());
                    }
                    _ => {}
                }
                inventory.entries.push(entry);
                if inventory.entries.len() == BATCH_SIZE && !cancel.is_cancelled() {
                    inventory.commit()?;
                }
            }
            Err(issue) => {
                let mut relative = issue.path.strip_prefix(&root).unwrap_or(Path::new(""));
                while relative.to_str().is_none() {
                    relative = relative.parent().unwrap_or(Path::new(""));
                }
                coverage.exclude(
                    &relative
                        .to_str()
                        .unwrap_or_default()
                        .replace(std::path::MAIN_SEPARATOR, "/"),
                );
                coverage.incomplete = true;
                walk_failed = true;
                let message = if issue.at_root {
                    unavailable = true;
                    format!("Sorgente non accessibile: {}", issue.error)
                } else {
                    issue.error.to_string()
                };
                inventory.stats.errors_count += 1;
                inventory.sink.error(IndexerErrorEvent {
                    source_id,
                    message: message.clone(),
                    path: Some(issue.path.to_string_lossy().into_owned()),
                    recoverable: true,
                });
                walk_notices.push(format!("{}: {message}", issue.path.display()));
            }
        }
        Ok(())
    })
    .and_then(|()| {
        if !unavailable && !cancel.is_cancelled() {
            if !inventory.entries.is_empty() {
                inventory.commit()?;
            }
            if !cancel.is_cancelled() {
                let recorded = std::mem::take(&mut inventory.recorded);
                let legacy =
                    if coverage.incomplete && history.get("").is_some_and(|model| model.pack) {
                        legacy_root_children(db, source_id)?
                    } else {
                        BTreeSet::new()
                    };
                let grouping = group(&recorded, &coverage, &legacy, |path| {
                    history.get(path).is_some_and(|model| model.pack)
                });
                inventory.progress(inventory.stats.files_indexed);
                if !cancel.is_cancelled() {
                    inventory.sink.batch_updated(write_groups(
                        db, source_id, &grouping, &recorded, &history,
                    )?);
                    inventory.stats.models_indexed = grouping.models.len() as u64;
                }
                if !cancel.is_cancelled() {
                    let grouped = GroupedInventory {
                        recorded: &recorded,
                        grouping,
                        history: &history,
                        coverage: &coverage,
                    };
                    let outcome = finish(&mut inventory, &root, grouped)?;
                    unavailable = outcome.unavailable;
                    metadata_notices = outcome.notices;
                }
            }
        }
        Ok(())
    });
    if let Err(error) = result {
        let _ = db.mark_source_status(
            source_id,
            SourceStatus::Error,
            Some(&error.to_string()),
            false,
        );
        return Err(error);
    }
    inventory.stats.cancelled = cancel.is_cancelled();
    let status = if unavailable {
        SourceStatus::Unavailable
    } else if walk_failed {
        SourceStatus::Error
    } else {
        SourceStatus::Idle
    };
    let completed = !inventory.stats.cancelled && !unavailable && !walk_failed;
    let mut notices = walk_notices.into_messages();
    if opaque {
        notices.push(
            "Scansione parziale: cartelle Online-Only non enumerate; storico conservato.".into(),
        );
    }
    notices.extend(metadata_notices.into_messages());
    let last_error = (!notices.is_empty()).then(|| notices.join("\n"));
    db.mark_source_status(source_id, status, last_error.as_deref(), completed)?;
    Ok(inventory.stats)
}

struct GroupedInventory<'a> {
    recorded: &'a [InventoryEntry],
    grouping: Grouping<'a>,
    history: &'a ModelHistory,
    coverage: &'a WalkCoverage,
}

#[derive(Default)]
struct FinishOutcome {
    unavailable: bool,
    notices: Notices,
}

#[derive(Default)]
struct Notices {
    messages: Vec<String>,
    omitted: usize,
}

impl Notices {
    fn push(&mut self, message: String) {
        if self.messages.len() < 4 {
            self.messages.push(message);
        } else {
            self.omitted += 1;
        }
    }

    fn into_messages(mut self) -> Vec<String> {
        if self.omitted > 0 {
            self.messages
                .push(format!("Altri avvisi: {}", self.omitted));
        }
        self.messages
    }
}

struct Inventory<'a, S> {
    db: &'a Database,
    source_id: i64,
    sink: &'a mut S,
    entries: Vec<InventoryEntry>,
    recorded: Vec<InventoryEntry>,
    stats: Pass1Stats,
    current_item: String,
}

impl<S: PipelineSink> Inventory<'_, S> {
    fn commit(&mut self) -> Result<(), IpcError> {
        let batch = write_inventory_batch(self.db, self.source_id, &self.entries)?;
        self.stats.files_indexed += self
            .entries
            .iter()
            .filter(|entry| matches!(entry, InventoryEntry::File { .. }))
            .count() as u64;
        self.stats.models_indexed += batch.updated_model_ids.len() as u64;
        self.recorded.append(&mut self.entries);
        self.sink.batch_updated(batch);
        self.progress(0); // A streaming walk only knows the total when it finishes.
        Ok(())
    }

    fn finish(
        &mut self,
        root: &Path,
        inspector: &(impl EntryMetadataInspector + Sync),
        cancel: &CancellationToken,
        GroupedInventory {
            recorded,
            grouping,
            history,
            coverage,
        }: GroupedInventory<'_>,
    ) -> Result<FinishOutcome, IpcError> {
        let mut outcome = FinishOutcome::default();
        let cached = cached_analysis(self.db, self.source_id)?;
        let files: Vec<_> = recorded
            .iter()
            .filter_map(|entry| match entry {
                InventoryEntry::File {
                    rel_path,
                    ext,
                    metadata,
                    ..
                } => {
                    let cached = cached.get(rel_path).copied();
                    let zip = ext == "zip";
                    let cloud = metadata.flags.is_placeholder();
                    if let Some(count) = cached.flatten() {
                        self.stats.files_indexed += count as u64;
                    }
                    if (zip && cached.flatten().is_none()) || (!cloud && cached.is_none()) {
                        Some((rel_path.as_str(), zip, metadata, cached.is_some()))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        let total = files.len() as u64;
        let mut processed = 0;
        self.metadata_progress(processed, total, "");
        if !files.is_empty() && !cancel.is_cancelled() {
            let workers = thread::available_parallelism().map_or(1, |n| n.get().min(8));
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .map_err(|_| IpcError::InternalError)?;
            let mut updates = Vec::with_capacity(BATCH_SIZE);
            let mut rows = 0;
            for chunk in files.chunks(workers) {
                if cancel.is_cancelled() {
                    break;
                }
                let batch = pool.install(|| {
                    chunk
                        .par_iter()
                        .filter_map(|&(path, zip, metadata, cached)| {
                            (!cancel.is_cancelled()).then(|| {
                                analyze(root, path, zip, metadata, cached, inspector, cancel)
                            })
                        })
                        .collect::<Vec<_>>()
                });
                for (result, error) in batch {
                    if cancel.is_cancelled() {
                        break;
                    }
                    if let Some(error) = error {
                        self.report_error(root, result.path, error, &mut outcome);
                    }
                    processed += 1;
                    self.metadata_progress(processed, total, result.path);
                    rows += 1 + result.zip.as_ref().map_or(0, Vec::len);
                    updates.push(result);
                    if rows >= BATCH_SIZE && !cancel.is_cancelled() {
                        self.flush_analysis(&mut updates)?;
                        rows = 0;
                    }
                }
            }
            if !updates.is_empty() && !cancel.is_cancelled() {
                self.flush_analysis(&mut updates)?;
            }
        }
        if !cancel.is_cancelled() {
            // Recheck availability before interpreting unobserved paths as deletions.
            if let Err(error) =
                check_source_directory(root, inspector).and_then(|()| fs::read_dir(root))
            {
                self.report_error(
                    root,
                    "",
                    format!("Sorgente non accessibile: {error}"),
                    &mut outcome,
                );
                outcome.unavailable = true;
            } else if !cancel.is_cancelled() {
                self.sink.batch_updated(finish_scan(
                    self.db,
                    self.source_id,
                    recorded,
                    &grouping,
                    history,
                    coverage,
                )?);
            }
        }
        Ok(outcome)
    }

    fn flush_analysis(&mut self, updates: &mut Vec<FileAnalysis<'_>>) -> Result<(), IpcError> {
        let event = write_analysis(self.db, self.source_id, updates)?;
        self.stats.files_indexed += updates
            .iter()
            .filter_map(|result| result.zip.as_ref())
            .map(|entries| entries.len() as u64)
            .sum::<u64>();
        updates.clear();
        if !event.updated_model_ids.is_empty() {
            self.sink.batch_updated(event);
        }
        Ok(())
    }

    fn report_error(
        &mut self,
        root: &Path,
        path: &str,
        message: String,
        outcome: &mut FinishOutcome,
    ) {
        self.stats.errors_count += 1;
        self.sink.error(IndexerErrorEvent {
            source_id: self.source_id,
            message: message.clone(),
            path: Some(root.join(path).to_string_lossy().into_owned()),
            recoverable: true,
        });
        outcome
            .notices
            .push(format!("{}: {message}", root.join(path).display()));
    }

    fn metadata_progress(&mut self, processed: u64, total: u64, path: &str) {
        self.sink.progress(IndexerProgressEvent {
            source_id: self.source_id,
            pass: IndexerPass::Metadata,
            phase_label: METADATA_LABEL.to_owned(),
            processed,
            total,
            current_item: path.to_owned(),
        });
    }

    fn progress(&mut self, total: u64) {
        self.sink.progress(IndexerProgressEvent {
            source_id: self.source_id,
            pass: IndexerPass::Inventory,
            phase_label: INVENTORY_LABEL.to_owned(),
            processed: self.stats.files_indexed,
            total,
            current_item: self.current_item.clone(),
        });
    }
}

fn analyze<'a>(
    root: &Path,
    path: &'a str,
    zip: bool,
    metadata: &EntryMetadata,
    cached: bool,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
) -> (FileAnalysis<'a>, Option<String>) {
    let mut result = FileAnalysis {
        path,
        hash: None,
        zip: None,
        complete: false,
    };
    if metadata.flags.is_placeholder() {
        result.zip = zip.then(Vec::new);
        return (result, None);
    }
    let analysis = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut file = open_resident(&root.join(path), metadata, inspector)?;
        if !cached {
            result.hash = Some(content_hash(&mut file, cancel)?);
        }
        if zip {
            file.rewind()?;
            match read_zip_directory(file) {
                Ok(entries) => result.zip = Some(entries),
                Err(error) => {
                    result.zip = Some(Vec::new());
                    return Err(error.into());
                }
            }
        }
        result.complete = true;
        Ok(())
    })();
    let error = analysis.err().map(|error| {
        if zip {
            format!("Archivio ZIP non leggibile: {error}")
        } else {
            format!("Impronta non calcolata: {error}")
        }
    });
    (result, error)
}
