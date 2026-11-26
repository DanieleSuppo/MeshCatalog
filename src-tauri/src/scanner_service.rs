use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use meshcatalog_core::{
    contracts::{
        IndexerBatchUpdatedEvent, IndexerCompletedEvent, IndexerErrorEvent, IndexerProgressEvent,
        ScanPass, ScanStatusDto,
    },
    db::Database,
    error::IpcError,
    pipeline::{run_scan, PipelineSink, INVENTORY_LABEL},
    scanner::attr_guard::OsMetadataInspector,
};
use tokio_util::sync::CancellationToken;

const RECENT_ERROR_LIMIT: usize = 20;

#[derive(Clone)]
pub(crate) struct ScannerService(Arc<Service>);

struct Service {
    db: Arc<Database>,
    // Serialize source selection and removal without blocking progress/status readers on the DB.
    source_gate: Mutex<()>,
    state: Mutex<ScanState>,
    released: Condvar,
}

#[derive(Default)]
struct ScanState {
    active: BTreeMap<i64, ActiveSource>,
    recent_errors: VecDeque<IndexerErrorEvent>,
}

struct ActiveSource {
    token: CancellationToken,
    progress: Option<IndexerProgressEvent>,
}

pub(crate) enum ScanEvent {
    Progress(IndexerProgressEvent),
    BatchUpdated(IndexerBatchUpdatedEvent),
    Error(IndexerErrorEvent),
    Completed(IndexerCompletedEvent),
}

impl ScannerService {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        Self(Arc::new(Service {
            db,
            source_gate: Mutex::new(()),
            state: Mutex::default(),
            released: Condvar::new(),
        }))
    }

    pub(crate) fn start(
        &self,
        source_id: Option<i64>,
        emit: impl FnMut(ScanEvent) + Send + 'static,
    ) -> Result<(), IpcError> {
        use meshcatalog_core::contracts::SourceKind;

        let _gate = self
            .0
            .source_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let sources = self
            .0
            .db
            .list_sources()?
            .into_iter()
            .filter(|s| s.kind == SourceKind::LocalDir);
        let ids: Vec<_> = sources
            .map(|s| s.id)
            .filter(|id| source_id.is_none_or(|selected| selected == *id))
            .collect();
        if source_id.is_some() && ids.is_empty() {
            return Err(IpcError::SourceNotFound);
        }
        let mut state = self.state();
        if ids.iter().any(|id| state.active.contains_key(id)) {
            return Err(IpcError::ScanAlreadyRunning);
        }
        if state.active.is_empty() {
            state.recent_errors.clear();
        }
        let mut pending = BTreeMap::new();
        for id in ids {
            let token = CancellationToken::new();
            pending.insert(id, token.clone());
            state.active.insert(
                id,
                ActiveSource {
                    token,
                    progress: None,
                },
            );
        }
        drop(state);
        let job = Job {
            service: self.clone(),
            pending,
        };
        tauri::async_runtime::spawn_blocking(move || job.run(source_id, emit));
        Ok(())
    }

    pub(crate) fn cancel(&self, source_id: Option<i64>) {
        for (&id, source) in &self.state().active {
            if source_id.is_none_or(|selected| selected == id) {
                source.token.cancel();
            }
        }
    }

    pub(crate) fn remove(&self, source_id: i64) -> Result<(), IpcError> {
        let _gate = self
            .0
            .source_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.cancel(Some(source_id));
        let state = self
            .0
            .released
            .wait_while(self.state(), |state| state.active.contains_key(&source_id))
            .unwrap_or_else(PoisonError::into_inner);
        drop(state);
        self.0.db.remove_source(source_id)
    }

    pub(crate) fn status(&self) -> ScanStatusDto {
        let state = self.state();
        let current = state
            .active
            .iter()
            .find(|(_, active)| active.progress.is_some())
            .or_else(|| state.active.first_key_value());
        let progress = current.and_then(|(_, active)| active.progress.as_ref());
        ScanStatusDto {
            is_running: current.is_some(),
            active_source_id: current.map(|(&id, _)| id),
            pass: current.map(|_| match progress.map(|p| p.pass) {
                Some(meshcatalog_core::contracts::IndexerPass::Metadata) => ScanPass::Metadata,
                _ => ScanPass::Inventory,
            }),
            phase_label: current.map(|_| {
                progress.map_or_else(|| INVENTORY_LABEL.to_owned(), |p| p.phase_label.clone())
            }),
            processed: progress.map_or(0, |p| p.processed),
            total: progress.map_or(0, |p| p.total),
            current_item: progress
                .filter(|p| !p.current_item.is_empty())
                .map(|p| p.current_item.clone()),
            recent_errors: state.recent_errors.iter().cloned().collect(),
        }
    }

    fn state(&self) -> MutexGuard<'_, ScanState> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Job {
    service: ScannerService,
    pending: BTreeMap<i64, CancellationToken>,
}

impl Job {
    fn run(mut self, source_id: Option<i64>, emit: impl FnMut(ScanEvent)) {
        let start = Instant::now();
        let mut sink = EventSink {
            service: self.service.clone(),
            emit,
            completed: IndexerCompletedEvent {
                source_id,
                cancelled: false,
                duration_ms: 0,
                models_indexed: 0,
                files_indexed: 0,
                errors_count: 0,
            },
        };
        while let Some((&id, token)) = self.pending.first_key_value() {
            match run_scan(
                &self.service.0.db,
                id,
                &OsMetadataInspector,
                token,
                &mut sink,
            ) {
                Ok(stats) => {
                    sink.completed.cancelled |= stats.cancelled;
                    sink.completed.models_indexed += stats.models_indexed;
                    sink.completed.files_indexed += stats.files_indexed;
                }
                Err(error) => sink.error(IndexerErrorEvent {
                    source_id: id,
                    message: error.to_string(),
                    path: None,
                    recoverable: false,
                }),
            }
            self.pending.remove(&id);
            self.service.state().active.remove(&id);
            self.service.0.released.notify_all();
        }
        sink.completed.duration_ms = start.elapsed().as_millis() as u64;
        (sink.emit)(ScanEvent::Completed(sink.completed));
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // Release reservations even if the background worker unwinds.
        let mut state = self.service.state();
        for id in self.pending.keys() {
            state.active.remove(id);
        }
        self.service.0.released.notify_all();
    }
}

struct EventSink<F> {
    service: ScannerService,
    emit: F,
    completed: IndexerCompletedEvent,
}

impl<F: FnMut(ScanEvent)> PipelineSink for EventSink<F> {
    fn progress(&mut self, event: IndexerProgressEvent) {
        if let Some(source) = self.service.state().active.get_mut(&event.source_id) {
            source.progress = Some(event.clone());
        }
        (self.emit)(ScanEvent::Progress(event));
    }

    fn batch_updated(&mut self, event: IndexerBatchUpdatedEvent) {
        (self.emit)(ScanEvent::BatchUpdated(event));
    }

    fn error(&mut self, event: IndexerErrorEvent) {
        self.completed.errors_count += 1;
        {
            let mut state = self.service.state();
            if state.recent_errors.len() == RECENT_ERROR_LIMIT {
                state.recent_errors.pop_front();
            }
            state.recent_errors.push_back(event.clone());
        }
        (self.emit)(ScanEvent::Error(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::mpsc, time::Duration};

    #[test]
    fn scan_all_rejects_an_overlapping_run_without_reserving_other_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&tmp.path().join("catalog.db")).unwrap());
        let root = tmp.path().canonicalize().unwrap();
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("model.stl"), []).unwrap();
        fs::write(b.join("other.obj"), []).unwrap();
        let a = db.add_local_source(&a, None).unwrap();
        let b = db.add_local_source(&b, None).unwrap();
        let service = ScannerService::new(db.clone());
        let (first_tx, first_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let mut first_progress = true;
        service
            .start(Some(a), move |event| {
                if matches!(event, ScanEvent::Progress(_)) && std::mem::take(&mut first_progress) {
                    first_tx.send(event).unwrap();
                    resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                } else {
                    first_tx.send(event).unwrap();
                }
            })
            .unwrap();
        assert!(matches!(
            first_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            ScanEvent::Progress(_)
        ));
        assert_eq!(service.status().active_source_id, Some(a));
        assert!(matches!(
            service.start(None, |_| {}),
            Err(IpcError::ScanAlreadyRunning)
        ));
        let (second_tx, second_rx) = mpsc::channel();
        service
            .start(Some(b), move |event| second_tx.send(event).unwrap())
            .unwrap();
        service.cancel(Some(a));
        resume_tx.send(()).unwrap();
        for (rx, id, cancelled) in [(first_rx, a, true), (second_rx, b, false)] {
            loop {
                if let ScanEvent::Completed(event) =
                    rx.recv_timeout(Duration::from_secs(5)).unwrap()
                {
                    assert_eq!(event.source_id, Some(id));
                    assert_eq!(event.cancelled, cancelled);
                    assert_eq!(event.files_indexed, u64::from(!cancelled));
                    break;
                }
            }
        }
        assert!(!service.status().is_running);
    }

    #[test]
    fn scan_all_survives_unavailable_source_and_reports_committed_inventory() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let db = Arc::new(Database::open(&root.join("catalog.db")).unwrap());
        let missing = root.join("missing");
        let resident = root.join("resident");
        fs::create_dir(&missing).unwrap();
        fs::create_dir(&resident).unwrap();
        fs::write(resident.join("model.stl"), []).unwrap();
        let unavailable_id = db.add_local_source(&missing, None).unwrap();
        let resident_id = db.add_local_source(&resident, None).unwrap();
        fs::remove_dir(&missing).unwrap();
        let service = ScannerService::new(db.clone());
        let (tx, rx) = mpsc::channel();
        service
            .start(None, move |event| tx.send(event).unwrap())
            .unwrap();

        let mut saw_unavailable = false;
        let mut saw_batch = false;
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                ScanEvent::Error(event) => {
                    assert_eq!(event.source_id, unavailable_id);
                    assert!(event.recoverable);
                    saw_unavailable = true;
                }
                ScanEvent::BatchUpdated(event) => {
                    assert_eq!(event.source_id, resident_id);
                    assert_eq!(db.list_sources().unwrap()[1].file_count, 1);
                    saw_batch = true;
                }
                ScanEvent::Completed(event) => {
                    assert_eq!(event.source_id, None);
                    assert_eq!(
                        (
                            event.models_indexed,
                            event.files_indexed,
                            event.errors_count
                        ),
                        (1, 1, 1)
                    );
                    break;
                }
                ScanEvent::Progress(_) => {}
            }
        }
        assert!(saw_unavailable && saw_batch);
        let status = service.status();
        assert!(!status.is_running);
        assert_eq!(status.recent_errors.len(), 1);
        service.remove(resident_id).unwrap();
        assert!(resident.join("model.stl").is_file());
        assert_eq!(db.list_sources().unwrap().len(), 1);
    }
}
