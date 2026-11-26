use std::{
    fs, io,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use meshcatalog_core::{
    contracts::{IndexerBatchUpdatedEvent, IndexerErrorEvent, IndexerProgressEvent, SourceStatus},
    db::Database,
    error::IpcError,
    pipeline::{run_pass1, run_scan, PipelineSink},
    scanner::{
        attr_guard::{
            EntryMetadata, EntryMetadataInspector, MockMetadataInspector, OsMetadataInspector,
            RawWin32AttrFlags,
        },
        exclusions::supported_extension,
    },
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Events {
    batches: Vec<IndexerBatchUpdatedEvent>,
    progress: Vec<IndexerProgressEvent>,
    errors: Vec<IndexerErrorEvent>,
    cancel_on_batch: Option<CancellationToken>,
}

impl PipelineSink for Events {
    fn progress(&mut self, event: IndexerProgressEvent) {
        self.progress.push(event);
    }

    fn batch_updated(&mut self, event: IndexerBatchUpdatedEvent) {
        self.batches.push(event);
        if let Some(token) = &self.cancel_on_batch {
            token.cancel();
        }
    }

    fn error(&mut self, event: IndexerErrorEvent) {
        self.errors.push(event);
    }
}

fn fixture() -> (TempDir, Database, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("catalog.db")).unwrap();
    let root = dir.path().join("library");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    (dir, db, root)
}

#[test]
fn recall_bits_independently_classify_placeholders_but_reparse_alone_does_not() {
    for (bits, expected) in [
        (0, false),
        (0x0400, false),
        (0x1000, true),
        (0x0004_0000, true),
        (0x0040_0000, true),
        (0x0040_0400, true),
    ] {
        let flags = RawWin32AttrFlags {
            file_attributes: bits,
            is_symlink: false,
            reparse_tag: None,
        };
        assert_eq!(flags.is_placeholder(), expected, "mask {bits:#x}");
    }
}

#[test]
fn registry_accepts_only_supported_extensions_case_insensitively() {
    for ext in [
        "stl", "3mf", "lys", "lyt", "obj", "ply", "glb", "step", "stp", "gcode", "zip", "jpg",
        "jpeg", "png", "webp",
    ] {
        assert_eq!(
            supported_extension(Path::new(&format!("test.{}", ext.to_uppercase()))).as_deref(),
            Some(ext)
        );
    }
    assert!(supported_extension(Path::new("test.zip.bak")).is_none());
}

#[test]
fn source_registration_rejects_exact_nested_and_ancestor_paths_by_component() {
    let (dir, db, root) = fixture();
    let child = root.join("child");
    let sibling = dir.path().join("library-more");
    fs::create_dir(&child).unwrap();
    fs::create_dir(&sibling).unwrap();
    let id = db.add_local_source(&root, None).unwrap();
    for path in [&root, &child, dir.path(), &root.join(".")] {
        assert!(
            matches!(
                db.add_local_source(path, None),
                Err(IpcError::SourceOverlap)
            ),
            "{path:?}"
        );
    }
    db.add_local_source(&sibling, Some("Sibling")).unwrap();
    let sources = db.list_sources().unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources
        .iter()
        .any(|source| source.id == id && source.name == "library"));
    assert!(matches!(
        db.add_local_source(&root.join("missing"), None),
        Err(IpcError::InvalidPath)
    ));
}

#[cfg(unix)]
#[test]
fn source_registration_does_not_follow_symlink_with_trailing_separator() {
    let (dir, db, root) = fixture();
    fs::create_dir(root.join("child")).unwrap();
    std::os::unix::fs::symlink(&root, dir.path().join("alias")).unwrap();
    for path in ["alias", "alias/", "alias/./", "alias/child"] {
        assert!(
            matches!(
                db.add_local_source(&dir.path().join(path), None),
                Err(IpcError::InvalidPath)
            ),
            "{path}"
        );
    }
    assert!(db.list_sources().unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn native_junction_is_rejected_as_root_or_ancestor_and_not_walked() {
    let (dir, db, root) = fixture();
    let external = dir.path().join("external");
    fs::create_dir_all(external.join("child")).unwrap();
    fs::write(external.join("child/outside.stl"), b"outside").unwrap();
    // Keep mklink's arguments relative to the private fixture, not a user's library.
    let output = std::process::Command::new("cmd.exe")
        .current_dir(dir.path())
        .args(["/d", "/c", "mklink", "/J", r"library\junction", "external"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let junction = root.join("junction");
    let metadata = OsMetadataInspector.inspect(&junction).unwrap();
    assert_eq!(metadata.flags.reparse_tag, Some(0xa000_0003));
    assert!(db.add_local_source(&junction, None).is_err());
    assert!(db.add_local_source(&junction.join("child"), None).is_err());
    let id = db.add_local_source(&root, None).unwrap();
    fs::write(root.join("inside.stl"), b"inside").unwrap();
    let stats = run_scan(
        &db,
        id,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!((stats.files_indexed, stats.errors_count), (1, 0));
}

#[test]
fn metadata_only_pipeline_indexes_placeholders_and_cascades_without_writing_source() {
    let (dir, db, root) = fixture();
    fs::create_dir(root.join("Créatures")).unwrap();
    fs::write(root.join("Créatures/Dragon.STL"), b"resident").unwrap();
    // Not a ZIP: any attempted Central Directory parsing would report an error.
    let zip = root.join("placeholder-only.zip");
    fs::write(&zip, b"do not open").unwrap();
    fs::write(root.join("._missing.stl"), b"junk").unwrap();
    for name in [
        "$recycle.bin",
        "SYSTEM VOLUME INFORMATION",
        ".DropBox.Cache",
        ".GIT",
        "Thumbs.DB",
        ".DS_Store",
        "__macosx",
    ] {
        fs::create_dir(root.join(name)).unwrap();
        fs::write(root.join(name).join("hidden.stl"), b"junk").unwrap();
    }
    fs::write(dir.path().join("outside.stl"), b"outside").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path(), root.join("link")).unwrap();
    let inspector = MockMetadataInspector::new([(
        zip.clone(),
        RawWin32AttrFlags {
            file_attributes: 0x0040_0400,
            is_symlink: false,
            reparse_tag: Some(0x9000_001a),
        },
    )]);
    let id = db.add_local_source(&root, Some("Test Library")).unwrap();
    let mut events = Events::default();
    let stats = run_pass1(&db, id, &inspector, &CancellationToken::new(), &mut events).unwrap();
    assert_eq!(
        (
            stats.models_indexed,
            stats.files_indexed,
            stats.errors_count
        ),
        (2, 2, 0)
    );
    assert!(!stats.cancelled);
    assert_eq!(events.batches.len(), 2); // Streaming inventory, then definitive grouping.
    assert_eq!(events.batches[0].updated_folder_ids.len(), 2);
    assert_eq!(events.batches[0].updated_model_ids.len(), 2);
    assert_eq!(events.progress.last().unwrap().processed, 2);
    assert!(events.errors.is_empty());
    // A repeated Pass 1 must not duplicate rows or FTS entries.
    run_pass1(
        &db,
        id,
        &inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    let source = db.list_sources().unwrap().pop().unwrap();
    assert_eq!(
        (source.model_count, source.file_count, source.status),
        (2, 2, SourceStatus::Idle)
    );
    assert!(source.last_scan_at.is_some());
    db.with_reader(|conn| {
        let actual: Vec<(String, String, i64, Option<String>, i64)> = conn.prepare(
            "SELECT filename, storage_state, index_pass, content_hash, size_bytes FROM files ORDER BY filename"
        )?.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
        assert_eq!(actual, vec![("Dragon.STL".into(), "local_resident".into(), 1, None, 8), ("placeholder-only.zip".into(), "cloud_placeholder".into(), 2, None, 11)]);
        let root_name: String = conn.query_row("SELECT name FROM folders WHERE parent_id IS NULL AND rel_path = ''", [], |r| r.get(0))?;
        assert_eq!(root_name, "Test Library");
        let cloud: (String, bool, Option<String>) = conn.query_row("SELECT container_kind, has_cloud_placeholder, fingerprint FROM models WHERE primary_ext = 'zip'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        assert_eq!(cloud, ("zip_archive".into(), true, None));
        for table in ["models_fts", "files_fts"] {
            let hits: i64 = conn.query_row(&format!("SELECT count(*) FROM {table} WHERE {table} MATCH 'creatures AND dragon AND stl'"), [], |r| r.get(0))?;
            assert_eq!(hits, 1, "{table}");
        }
        Ok(())
    }).unwrap();
    db.remove_source(id).unwrap();
    assert!(db.list_sources().unwrap().is_empty());
    db.with_reader(|conn| {
        for table in ["folders", "models", "files", "models_fts", "files_fts"] {
            let count: i64 =
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            assert_eq!(count, 0, "{table}");
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::metadata(zip).unwrap().len(), 11);
    assert_eq!(
        fs::read(root.join("Créatures/Dragon.STL")).unwrap(),
        b"resident"
    );
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        if cfg!(unix) { 11 } else { 10 }
    );
}

#[test]
fn missing_root_keeps_historical_inventory_and_emits_recoverable_error() {
    let (dir, db, root) = fixture();
    fs::write(root.join("model.3mf"), b"model").unwrap();
    let id = db.add_local_source(&root, None).unwrap();
    run_pass1(
        &db,
        id,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    let last_scan = db.list_sources().unwrap()[0].last_scan_at;
    fs::rename(&root, dir.path().join("disconnected")).unwrap();
    let mut events = Events::default();
    let stats = run_pass1(
        &db,
        id,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut events,
    )
    .unwrap();
    let source = db.list_sources().unwrap().pop().unwrap();
    assert_eq!(source.status, SourceStatus::Unavailable);
    assert_eq!(source.last_scan_at, last_scan);
    assert_eq!((source.model_count, source.file_count), (1, 1));
    assert!(source.last_error.is_some());
    assert_eq!(stats.errors_count, 1);
    assert!(events.errors[0].recoverable);
    assert!(events.batches.is_empty());
}

#[test]
fn unreadable_subtree_is_not_pruned_and_does_not_block_its_siblings() {
    struct DenySubtree;
    impl EntryMetadataInspector for DenySubtree {
        fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
            if path.ends_with("blocked") {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "blocked fixture",
                ));
            }
            let mut metadata = OsMetadataInspector.inspect(path)?;
            if path.ends_with("sibling.obj") {
                metadata.flags.file_attributes = 0x0400;
                metadata.flags.reparse_tag = Some(0x9000_301a);
            }
            Ok(metadata)
        }
    }

    let (_dir, db, root) = fixture();
    fs::create_dir(root.join("blocked")).unwrap();
    fs::write(root.join("blocked/history.stl"), []).unwrap();
    let id = db.add_local_source(&root, None).unwrap();
    let cancel = CancellationToken::new();
    run_scan(
        &db,
        id,
        &OsMetadataInspector,
        &cancel,
        &mut Events::default(),
    )
    .unwrap();
    let sibling = root.join("sibling.obj");
    fs::write(&sibling, b"resident reparse file").unwrap();
    let last_scan = db.list_sources().unwrap()[0].last_scan_at;

    let mut events = Events::default();
    let stats = run_scan(&db, id, &DenySubtree, &cancel, &mut events).unwrap();
    assert_eq!(stats.files_indexed, 1);
    db.with_reader(|conn| {
        let mut stmt = conn.prepare(
            "SELECT folders.rel_path, models.raw_name, files.filename, files.content_hash IS NOT NULL
             FROM files JOIN models ON models.id = files.model_id
             JOIN folders ON folders.id = models.folder_id ORDER BY files.filename",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<(String, String, String, bool)>, _>>()?;
        assert_eq!(
            rows,
            [
                ("blocked".into(), "history.stl".into(), "history.stl".into(), true),
                ("".into(), "sibling.obj".into(), "sibling.obj".into(), true),
            ]
        );
        Ok(())
    })
    .unwrap();
    let source = &db.list_sources().unwrap()[0];
    assert_eq!(source.status, SourceStatus::Error);
    assert_eq!(source.last_scan_at, last_scan);
    assert!(source.last_error.is_some());
    assert_eq!(events.errors.len(), 1);
    assert!(events.errors[0].recoverable);
    assert!(events.errors[0]
        .path
        .as_deref()
        .unwrap()
        .ends_with("blocked"));
}

#[test]
fn unknown_reparse_files_and_directories_are_not_opened_or_indexed() {
    let (_dir, db, root) = fixture();
    fs::write(root.join("unknown.stl"), b"must not hash").unwrap();
    fs::create_dir(root.join("unknown-dir")).unwrap();
    fs::write(root.join("unknown-dir/hidden.stl"), []).unwrap();
    let inspector = MockMetadataInspector::new(["unknown.stl", "unknown-dir"].map(|name| {
        (
            root.join(name),
            RawWin32AttrFlags {
                file_attributes: 0x0400,
                is_symlink: false,
                reparse_tag: None,
            },
        )
    }));
    let id = db.add_local_source(&root, None).unwrap();
    let stats = run_scan(
        &db,
        id,
        &inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(stats.files_indexed, 0);
    assert_eq!(db.list_sources().unwrap()[0].model_count, 0);
}

#[test]
fn opaque_directory_finishes_with_a_visible_notice() {
    let (_dir, db, root) = fixture();
    let cloud = root.join("Remote");
    fs::create_dir(&cloud).unwrap();
    fs::write(cloud.join("hidden.stl"), []).unwrap();
    let inspector = MockMetadataInspector::new([(
        cloud,
        RawWin32AttrFlags {
            file_attributes: 0x1000,
            is_symlink: false,
            reparse_tag: None,
        },
    )]);
    let id = db.add_local_source(&root, None).unwrap();
    run_scan(
        &db,
        id,
        &inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    let source = &db.list_sources().unwrap()[0];
    assert_eq!((source.file_count, source.status), (0, SourceStatus::Idle));
    assert!(source.last_scan_at.is_some());
    assert!(source
        .last_error
        .as_deref()
        .unwrap()
        .contains("Online-Only"));
}

#[test]
fn root_unavailable_at_prune_time_keeps_historical_inventory() {
    struct DisconnectAfterWalk<'a>(&'a Path, AtomicBool);
    impl EntryMetadataInspector for DisconnectAfterWalk<'_> {
        fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
            if path == self.0 && self.1.swap(true, Ordering::Relaxed) {
                return Err(io::ErrorKind::NotFound.into());
            }
            OsMetadataInspector.inspect(path)
        }
    }

    let (_dir, db, root) = fixture();
    let file = root.join("history.stl");
    fs::write(&file, []).unwrap();
    let id = db.add_local_source(&root, None).unwrap();
    let cancel = CancellationToken::new();
    run_scan(
        &db,
        id,
        &OsMetadataInspector,
        &cancel,
        &mut Events::default(),
    )
    .unwrap();
    let last_scan = db.list_sources().unwrap()[0].last_scan_at;
    // The walk sees no files, so the next root check is immediately before pruning.
    fs::remove_file(file).unwrap();
    let mut events = Events::default();
    run_scan(
        &db,
        id,
        &DisconnectAfterWalk(&root, AtomicBool::new(false)),
        &cancel,
        &mut events,
    )
    .unwrap();
    let source = &db.list_sources().unwrap()[0];
    assert_eq!((source.model_count, source.file_count), (1, 1));
    assert_eq!(source.status, SourceStatus::Unavailable);
    assert_eq!(source.last_scan_at, last_scan);
    assert_eq!(events.errors.len(), 1);
    assert!(events.errors[0].recoverable);
}

#[test]
fn cancellation_after_committed_batch_stops_before_the_next_batch() {
    let (_dir, db, root) = fixture();
    for i in 0..510 {
        fs::write(root.join(format!("model{i}.stl")), []).unwrap();
    }
    let id = db.add_local_source(&root, None).unwrap();
    let token = CancellationToken::new();
    let mut events = Events {
        cancel_on_batch: Some(token.clone()),
        ..Events::default()
    };
    let stats = run_pass1(&db, id, &OsMetadataInspector, &token, &mut events).unwrap();
    assert!(stats.cancelled);
    assert_eq!(events.batches.len(), 1);
    assert_eq!(stats.files_indexed, 499); // 500 records including the root directory.
    let source = db.list_sources().unwrap().pop().unwrap();
    assert_eq!(source.status, SourceStatus::Idle);
    assert_eq!(source.last_scan_at, None);
    assert_eq!(source.file_count, 499);
}
