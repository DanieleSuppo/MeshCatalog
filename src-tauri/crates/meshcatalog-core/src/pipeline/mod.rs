//! Background indexing passes.

use std::{fs, io::Seek, path::Path, thread};

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
            cached_analysis, finish_scan, scan_history, write_analysis, write_folder_aggregates,
            write_groups, write_inventory_batch, FileAnalysis, ModelHistory,
        },
        Database,
    },
    error::IpcError,
    hasher::content_hash,
    scanner::{
        attr_guard::{open_resident, EntryMetadata, EntryMetadataInspector},
        grouping::group,
        walker::{check_source_directory, walk_inventory, InventoryEntry},
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
    run(db, source_id, inspector, cancel, sink, false, |_, _, _| {
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
        true,
        |inventory, root, complete_walk| inventory.finish(root, inspector, cancel, complete_walk),
    )
}

fn run<S: PipelineSink>(
    db: &Database,
    source_id: i64,
    inspector: &impl EntryMetadataInspector,
    cancel: &CancellationToken,
    sink: &mut S,
    include_metadata: bool,
    finish: impl FnOnce(&mut Inventory<'_, S>, &Path, bool) -> Result<FinishOutcome, IpcError>,
) -> Result<Pass1Stats, IpcError> {
    let root = db.local_source_root(source_id)?;
    if cancel.is_cancelled() {
        return Ok(Pass1Stats {
            cancelled: true,
            ..Pass1Stats::default()
        });
    }
    let history = include_metadata
        .then(|| scan_history(db, source_id))
        .transpose()?;
    db.mark_source_status(source_id, SourceStatus::Scanning, None, false)?;
    let mut inventory = Inventory {
        db,
        source_id,
        sink,
        entries: Vec::with_capacity(BATCH_SIZE),
        recorded: include_metadata.then(Vec::new),
        history,
        stats: Pass1Stats::default(),
        current_item: String::new(),
    };
    inventory.progress(0);
    let mut unavailable = false;
    let mut complete_walk = true;
    let mut walk_failed = false;
    let mut last_error = None;
    let result = walk_inventory(&root, inspector, cancel, |entry| {
        match entry {
            Ok(entry) => {
                if let InventoryEntry::File { rel_path, .. } = &entry {
                    inventory.current_item.clone_from(rel_path);
                }
                if matches!(entry, InventoryEntry::Directory { opaque: true, .. }) {
                    complete_walk = false;
                }
                inventory.entries.push(entry);
                if inventory.entries.len() == BATCH_SIZE && !cancel.is_cancelled() {
                    inventory.commit()?;
                }
            }
            Err(issue) => {
                complete_walk = false;
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
                last_error = Some(message);
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
                let outcome = finish(&mut inventory, &root, complete_walk)?;
                unavailable = outcome.unavailable;
                last_error = outcome.last_error.or(last_error.take());
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
    let completed =
        !inventory.stats.cancelled && !unavailable && !walk_failed && last_error.is_none();
    if completed && !complete_walk {
        last_error = Some(
            "Scansione parziale: cartelle Online-Only non enumerate; storico conservato.".into(),
        );
    }
    db.mark_source_status(source_id, status, last_error.as_deref(), completed)?;
    if completed && !include_metadata {
        inventory.progress(inventory.stats.files_indexed);
    }
    Ok(inventory.stats)
}

#[derive(Default)]
struct FinishOutcome {
    unavailable: bool,
    last_error: Option<String>,
}

struct Inventory<'a, S> {
    db: &'a Database,
    source_id: i64,
    sink: &'a mut S,
    entries: Vec<InventoryEntry>,
    recorded: Option<Vec<InventoryEntry>>,
    history: Option<ModelHistory>,
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
        if let Some(recorded) = &mut self.recorded {
            recorded.append(&mut self.entries);
        } else {
            self.entries.clear();
        }
        self.sink.batch_updated(batch);
        self.progress(0); // A streaming walk only knows the total when it finishes.
        Ok(())
    }

    fn finish(
        &mut self,
        root: &Path,
        inspector: &(impl EntryMetadataInspector + Sync),
        cancel: &CancellationToken,
        complete_walk: bool,
    ) -> Result<FinishOutcome, IpcError> {
        let mut outcome = FinishOutcome::default();
        let recorded = self.recorded.take().expect("full inventory enabled");
        let history = self.history.take().expect("full inventory history enabled");
        let grouping = group(&recorded);
        self.progress(self.stats.files_indexed);
        if cancel.is_cancelled() {
            return Ok(outcome);
        }
        self.sink
            .batch_updated(write_groups(self.db, self.source_id, &grouping)?);
        self.stats.models_indexed = grouping.models.len() as u64;
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
        let mut zip_changed = false;
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
                        zip_changed |= self.flush_analysis(&mut updates)?;
                        rows = 0;
                    }
                }
            }
            if !updates.is_empty() && !cancel.is_cancelled() {
                zip_changed |= self.flush_analysis(&mut updates)?;
            }
        }
        if !cancel.is_cancelled() && complete_walk {
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
                    &recorded,
                    &grouping,
                    &history,
                )?);
                zip_changed = false;
            }
        }
        if zip_changed {
            self.sink
                .batch_updated(write_folder_aggregates(self.db, self.source_id)?);
        }
        Ok(outcome)
    }

    fn flush_analysis(&mut self, updates: &mut Vec<FileAnalysis<'_>>) -> Result<bool, IpcError> {
        let event = write_analysis(self.db, self.source_id, updates)?;
        self.stats.files_indexed += updates
            .iter()
            .filter_map(|result| result.zip.as_ref())
            .map(|entries| entries.len() as u64)
            .sum::<u64>();
        updates.clear();
        let visible_change = !event.updated_model_ids.is_empty();
        if visible_change {
            self.sink.batch_updated(event);
        }
        Ok(visible_change)
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
        outcome.last_error = Some(message);
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
        zip: zip.then(Vec::new),
        complete: false,
    };
    if metadata.flags.is_placeholder() {
        return (result, None);
    }
    let analysis = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut file = open_resident(&root.join(path), metadata, inspector)?;
        if !cached {
            result.hash = Some(content_hash(&mut file, cancel)?);
        }
        if zip {
            file.rewind()?;
            result.zip = Some(read_zip_directory(file)?);
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
