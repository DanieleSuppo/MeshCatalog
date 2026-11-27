use meshcatalog_core::{
    contracts::{IndexerBatchUpdatedEvent, IndexerErrorEvent, IndexerProgressEvent},
    db::Database,
    pipeline::PipelineSink,
};
use std::{fs, path::PathBuf};

pub struct Fixture {
    _temp: tempfile::TempDir,
    pub root: PathBuf,
    pub db: Database,
    pub source: i64,
}

impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("catalog.db")).unwrap();
        let root = temp.path().join("library");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let source = db.add_local_source(&root, Some("Test library")).unwrap();
        Self {
            _temp: temp,
            root,
            db,
            source,
        }
    }

    pub fn file(&self, path: &str, bytes: &[u8]) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

#[derive(Default)]
pub struct Events {
    pub errors: Vec<IndexerErrorEvent>,
    pub progress: Vec<IndexerProgressEvent>,
}

impl PipelineSink for Events {
    fn progress(&mut self, event: IndexerProgressEvent) {
        self.progress.push(event);
    }
    fn batch_updated(&mut self, _: IndexerBatchUpdatedEvent) {}
    fn error(&mut self, event: IndexerErrorEvent) {
        self.errors.push(event);
    }
}
