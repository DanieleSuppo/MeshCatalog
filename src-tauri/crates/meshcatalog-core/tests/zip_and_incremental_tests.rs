mod common;

use common::{Events, Fixture};
use meshcatalog_core::{
    contracts::{
        IndexerBatchUpdatedEvent, IndexerErrorEvent, IndexerPass, IndexerProgressEvent,
        SearchItemDto, SearchQuery, SourceFilter, ViewMode,
    },
    pipeline::{run_scan, PipelineSink},
    scanner::attr_guard::{
        EntryMetadata, EntryMetadataInspector, MockMetadataInspector, OsMetadataInspector,
        RawWin32AttrFlags,
    },
};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::{Duration, SystemTime},
};
use tokio_util::sync::CancellationToken;
use zip::{write::SimpleFileOptions, ZipWriter};

fn zip(path: &Path, names: &[&str]) {
    let mut archive = ZipWriter::new(fs::File::create(path).unwrap());
    for name in names {
        archive
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(b"payload").unwrap();
    }
    archive.finish().unwrap();
}

#[test]
fn archive_indexes_only_supported_safe_entries_and_survives_bad_archives() {
    let f = Fixture::new();
    let archive = f.root.join("2025_03_King-Thror_32mm.zip");
    zip(
        &archive,
        &[
            "inside/Throne.stl",
            "nested.zip",
            "photo.png",
            "inside/__MACOSX/ignored.stl",
            "inside/._ignored.stl",
            "readme.txt",
            "../escape.stl",
        ],
    );
    f.file("corrupt.zip", b"broken");
    f.file("cloud.zip", b"never parse");
    // Info-ZIP fixture: `zip -X -0 -P fixture protected.zip secret.stl`.
    f.file(
        "protected.zip",
        &[
            80, 75, 3, 4, 10, 0, 9, 0, 0, 0, 40, 156, 122, 93, 238, 64, 229, 5, 19, 0, 0, 0, 7, 0,
            0, 0, 10, 0, 0, 0, 115, 101, 99, 114, 101, 116, 46, 115, 116, 108, 50, 130, 220, 217,
            131, 213, 153, 112, 245, 20, 198, 56, 222, 80, 107, 138, 8, 196, 239, 80, 75, 7, 8,
            238, 64, 229, 5, 19, 0, 0, 0, 7, 0, 0, 0, 80, 75, 1, 2, 30, 3, 10, 0, 9, 0, 0, 0, 40,
            156, 122, 93, 238, 64, 229, 5, 19, 0, 0, 0, 7, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 180, 129, 0, 0, 0, 0, 115, 101, 99, 114, 101, 116, 46, 115, 116, 108, 80, 75, 5, 6,
            0, 0, 0, 0, 1, 0, 1, 0, 56, 0, 0, 0, 75, 0, 0, 0, 0, 0,
        ],
    );
    let contents = fs::read(&archive).unwrap();
    let inspector = MockMetadataInspector::new([(
        f.root.join("cloud.zip"),
        RawWin32AttrFlags {
            file_attributes: 0x1000,
            is_symlink: false,
        },
    )]);
    let mut sink = Events::default();
    let stats = run_scan(
        &f.db,
        f.source,
        &inspector,
        &CancellationToken::new(),
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        (
            stats.models_indexed,
            stats.files_indexed,
            stats.errors_count
        ),
        (4, 7, 2)
    );
    assert!(sink
        .progress
        .iter()
        .any(|e| e.pass == IndexerPass::Metadata));
    let mut errors: Vec<_> = sink
        .errors
        .iter()
        .map(|e| {
            assert!(e.recoverable);
            e.path.as_deref().unwrap()
        })
        .collect();
    errors.sort_unstable();
    assert!(errors[0].ends_with("corrupt.zip") && errors[1].ends_with("protected.zip"));
    assert_eq!(fs::read_dir(&f.root).unwrap().count(), 4);
    assert_eq!(fs::read(&archive).unwrap(), contents);
    f.db.with_reader(|db| {
        let mut s = db.prepare("SELECT e.rel_path, e.ext, e.size_bytes, e.zip_entry_csize, e.zip_entry_method, e.index_pass, e.uri, p.uri FROM files e JOIN files p ON p.id=e.parent_archive_id WHERE e.storage_state='zip_entry' ORDER BY e.rel_path")?;
        let expected = [("inside/Throne.stl", "stl"), ("nested.zip", "zip"), ("photo.png", "png")];
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?,r.get::<_, String>(1)?,r.get::<_, i64>(2)?,r.get::<_, i64>(3)?,r.get::<_, i64>(4)?,r.get::<_, i64>(5)?,r.get::<_, String>(6)?,r.get::<_, String>(7)?)))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(rows.len(), expected.len());
        for ((path, ext, size, csize, method, pass, uri, parent), (want_path, want_ext)) in rows.into_iter().zip(expected) {
            assert_eq!((path.as_str(), ext.as_str(), size, csize, method, pass), (want_path, want_ext, 7, 7, 0, 2));
            assert_eq!(uri, format!("zip://{parent}#{path}"));
        }
        assert_eq!(db.query_row("SELECT file_count,total_bytes FROM models WHERE display_title='King-Thror'", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?, (4, contents.len() as i64 + 21));
        for t in ["models_fts", "files_fts"] {
            assert_eq!(db.query_row(&format!("SELECT count(*) FROM {t} WHERE {t} MATCH 'throne'"), [], |r|r.get::<_,i64>(0))?, 1);
        }
        Ok(())
    }).unwrap();

    let mut query = SearchQuery {
        text: "throne".into(),
        view_mode: ViewMode::Files,
        source_filter: SourceFilter::All,
        source_id: None,
        folder_id: None,
        facets: Default::default(),
        limit: 20,
        offset: 0,
    };
    let result = f.db.search_catalog(&query).unwrap();
    let SearchItemDto::File(file) = &result.items[0] else {
        panic!("wrong view");
    };
    assert_eq!(
        file.rel_path,
        "2025_03_King-Thror_32mm.zip/inside/Throne.stl"
    );
    assert_eq!(
        file.parent_archive_name.as_deref(),
        Some("2025_03_King-Thror_32mm.zip")
    );
    let detail = f.db.get_model_detail(file.model_id.unwrap()).unwrap();
    assert_eq!(
        detail
            .files
            .iter()
            .map(|f| f.rel_path.as_str())
            .collect::<Vec<_>>(),
        [
            "2025_03_King-Thror_32mm.zip",
            "inside/Throne.stl",
            "nested.zip",
            "photo.png"
        ]
    );

    zip(&archive, &["new/Replacement.3mf", "photo.png"]);
    run_scan(
        &f.db,
        f.source,
        &inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(f.db.search_catalog(&query).unwrap().total_files, 0);
    assert_eq!(f.db.search_catalog(&query).unwrap().total_models, 0);
    query.text = "replacement".into();
    let replacement = f.db.search_catalog(&query).unwrap();
    assert_eq!((replacement.total_models, replacement.total_files), (1, 1));
    assert_eq!(f.db.get_model_detail(detail.id).unwrap().file_count, 3);
}

#[test]
fn metadata_is_checked_again_before_opening_a_previously_resident_zip() {
    struct BecomesPlaceholder(AtomicUsize);
    impl EntryMetadataInspector for BecomesPlaceholder {
        fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
            let mut metadata = OsMetadataInspector.inspect(path)?;
            if path.ends_with("changed.zip") {
                let seen = self.0.fetch_add(1, Relaxed);
                if seen > 0 {
                    metadata.flags.file_attributes = 0x0040_0000;
                }
            }
            Ok(metadata)
        }
    }
    let f = Fixture::new();
    zip(&f.root.join("changed.zip"), &["member.stl"]);
    let inspector = BecomesPlaceholder(AtomicUsize::new(0));
    let stats = run_scan(
        &f.db,
        f.source,
        &inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(
        (
            stats.models_indexed,
            stats.files_indexed,
            stats.errors_count
        ),
        (1, 1, 1)
    );
    assert_eq!(inspector.0.load(Relaxed), 2);
    assert_eq!(model(&f, "changed.zip").1, None);
}

fn scan(f: &Fixture, inspector: &(impl EntryMetadataInspector + Sync)) {
    run_scan(
        &f.db,
        f.source,
        inspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
}

fn model(f: &Fixture, name: &str) -> (i64, Option<String>) {
    f.db.with_reader(|db| {
        Ok(db.query_row(
            "SELECT id, fingerprint FROM models WHERE raw_name=?1",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })
    .unwrap()
}

fn count(f: &Fixture, sql: &str) -> i64 {
    f.db.with_reader(|db| Ok(db.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn unchanged_residents_never_reach_the_hash_open_guard() {
    struct CountInspections(AtomicUsize);
    impl EntryMetadataInspector for CountInspections {
        fn inspect(&self, path: &Path) -> io::Result<EntryMetadata> {
            if path.ends_with("mesh.stl") {
                self.0.fetch_add(1, Relaxed);
            }
            OsMetadataInspector.inspect(path)
        }
    }
    let f = Fixture::new();
    let inspector = CountInspections(AtomicUsize::new(0));
    let hash = || {
        f.db.with_reader(|db| {
            Ok(db.query_row(
                "SELECT content_hash FROM files WHERE filename='mesh.stl' AND index_pass=2",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap()
    };
    let set_time = |elapsed| {
        fs::File::options()
            .write(true)
            .open(f.root.join("mesh.stl"))
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(elapsed))
            .unwrap();
    };
    f.file("mesh.stl", b"first");
    set_time(1000);
    scan(&f, &inspector);
    assert_eq!(hash(), blake3::hash(b"first").to_hex().as_str());
    assert_eq!(inspector.0.swap(0, Relaxed), 2); // Inventory and guarded data open.
    scan(&f, &inspector);
    assert_eq!(inspector.0.swap(0, Relaxed), 1); // Inventory only.
    assert_eq!(hash(), blake3::hash(b"first").to_hex().as_str());
    f.file("mesh.stl", b"other");
    set_time(1001); // Same length, different mtime.
    scan(&f, &inspector);
    assert_eq!(inspector.0.swap(0, Relaxed), 2);
    assert_eq!(hash(), blake3::hash(b"other").to_hex().as_str());
    f.file("mesh.stl", b"longer");
    set_time(1001); // Same mtime, different length.
    scan(&f, &inspector);
    assert_eq!(inspector.0.load(Relaxed), 2);
    assert_eq!(hash(), blake3::hash(b"longer").to_hex().as_str());
}

#[test]
fn moving_pack_and_zip_preserves_model_ids_and_refreshes_both_search_indexes() {
    let f = Fixture::new();
    f.file("Obsidian/head.stl", b"head");
    f.file("Obsidian/body.stl", b"body");
    zip(&f.root.join("Garnet.zip"), &["inside/Throne.stl"]);
    scan(&f, &OsMetadataInspector);
    let pack = model(&f, "Obsidian");
    let archive = model(&f, "Garnet.zip");
    fs::create_dir(f.root.join("destination")).unwrap();
    fs::rename(f.root.join("Obsidian"), f.root.join("destination/Emerald")).unwrap();
    fs::rename(
        f.root.join("destination/Emerald/head.stl"),
        f.root.join("destination/Emerald/z_head.stl"),
    )
    .unwrap();
    fs::rename(
        f.root.join("Garnet.zip"),
        f.root.join("destination/Sapphire.zip"),
    )
    .unwrap();
    scan(&f, &OsMetadataInspector);
    assert_eq!(model(&f, "Emerald"), pack);
    assert_eq!(model(&f, "Sapphire.zip"), archive);
    assert!(pack.1.is_some() && archive.1.is_some());
    for table in ["models_fts", "files_fts"] {
        assert_eq!(
            count(
                &f,
                &format!("SELECT count(*) FROM {table} WHERE {table} MATCH 'obsidian OR garnet'")
            ),
            0
        );
        assert_eq!(
            count(
                &f,
                &format!("SELECT count(*) FROM {table} WHERE {table} MATCH 'sapphire AND throne'")
            ),
            1
        );
    }
    let detail = f.db.get_model_detail(archive.0).unwrap();
    assert!(detail.canonical_uri.ends_with("destination/Sapphire.zip"));
    let entry = detail
        .files
        .iter()
        .find(|file| file.rel_path == "inside/Throne.stl")
        .unwrap();
    let parent = detail
        .files
        .iter()
        .find(|file| file.rel_path == "Sapphire.zip")
        .unwrap();
    assert_eq!(entry.parent_archive_id, Some(parent.id));
    assert_eq!(count(&f, "SELECT count(*) FROM models"), 2);
    assert_eq!(count(&f, "SELECT count(*) FROM files"), 4);
}

#[test]
fn duplicate_and_null_fingerprints_do_not_reuse_model_ids() {
    let f = Fixture::new();
    for name in ["one/first.stl", "two/second.stl", "cloud.stl"] {
        f.file(name, b"identical");
    }
    let inspector = MockMetadataInspector::new(["cloud.stl", "moved-cloud.stl"].map(|p| {
        (
            f.root.join(p),
            RawWin32AttrFlags {
                file_attributes: 0x1000,
                is_symlink: false,
            },
        )
    }));
    scan(&f, &inspector);
    let first = model(&f, "first.stl");
    let second = model(&f, "second.stl");
    let cloud = model(&f, "cloud.stl");
    assert_eq!(first.1, second.1);
    assert!(first.1.is_some());
    assert_eq!(cloud.1, None);
    fs::remove_file(f.root.join("one/first.stl")).unwrap();
    fs::rename(f.root.join("two/second.stl"), f.root.join("two/copy.stl")).unwrap();
    fs::rename(f.root.join("cloud.stl"), f.root.join("moved-cloud.stl")).unwrap();
    scan(&f, &inspector);
    assert!(![first.0, second.0].contains(&model(&f, "copy.stl").0));
    let moved_cloud = model(&f, "moved-cloud.stl");
    assert_eq!(moved_cloud.1, None);
    assert_ne!(moved_cloud.0, cloud.0);
    assert_eq!(count(&f, "SELECT count(*) FROM models"), 2);
}

#[test]
fn accessible_deletions_prune_history_but_unavailable_root_does_not() {
    let f = Fixture::new();
    f.file("lost/a.stl", b"a");
    f.file("lost/b.stl", b"b");
    f.file("lost/photo.jpg", b"c");
    zip(&f.root.join("gone.zip"), &["entry.stl"]);
    scan(&f, &OsMetadataInspector);
    let parked = f.root.with_file_name("parked");
    fs::rename(&f.root, &parked).unwrap();
    scan(&f, &OsMetadataInspector);
    assert_eq!(count(&f, "SELECT count(*) FROM files"), 5);
    assert_eq!(
        count(
            &f,
            "SELECT count(*) FROM sources WHERE status='unavailable'"
        ),
        1
    );
    fs::rename(parked, &f.root).unwrap();
    fs::remove_dir_all(f.root.join("lost")).unwrap();
    fs::remove_file(f.root.join("gone.zip")).unwrap();
    scan(&f, &OsMetadataInspector);
    for table in ["models", "files", "models_fts", "files_fts"] {
        assert_eq!(
            count(&f, &format!("SELECT count(*) FROM {table}")),
            0,
            "{table}"
        );
    }
    assert_eq!(count(&f, "SELECT count(*) FROM folders"), 1);
}

#[test]
fn incomplete_or_cancelled_scans_cannot_prune_unobserved_history() {
    let f = Fixture::new();
    f.file("was/old.stl", b"old");
    f.file("opaque/hidden.stl", b"hidden");
    zip(&f.root.join("switch.zip"), &["former.stl"]);
    scan(&f, &OsMetadataInspector);
    assert!(model(&f, "switch.zip").1.is_some());
    let old = model(&f, "old.stl");
    let hidden = model(&f, "hidden.stl");
    fs::remove_file(f.root.join("was/old.stl")).unwrap();
    let inspector = MockMetadataInspector::new(["opaque", "switch.zip"].map(|name| {
        (
            f.root.join(name),
            RawWin32AttrFlags {
                file_attributes: 0x1000,
                is_symlink: false,
            },
        )
    }));
    scan(&f, &inspector);
    assert_eq!(model(&f, "old.stl"), old);
    assert_eq!(model(&f, "hidden.stl"), hidden);
    assert_eq!(model(&f, "switch.zip").1, None);
    assert_eq!(
        count(
            &f,
            "SELECT count(*) FROM files_fts WHERE files_fts MATCH 'former'"
        ),
        0
    );

    struct CancelAtPassTwo(CancellationToken);
    impl PipelineSink for CancelAtPassTwo {
        fn progress(&mut self, event: IndexerProgressEvent) {
            if event.pass == IndexerPass::Metadata {
                self.0.cancel();
            }
        }
        fn batch_updated(&mut self, _: IndexerBatchUpdatedEvent) {}
        fn error(&mut self, error: IndexerErrorEvent) {
            panic!("unexpected scan error: {}", error.message);
        }
    }
    let cancel = CancellationToken::new();
    let stats = run_scan(
        &f.db,
        f.source,
        &OsMetadataInspector,
        &cancel,
        &mut CancelAtPassTwo(cancel.clone()),
    )
    .unwrap();
    assert!(stats.cancelled);
    assert_eq!(model(&f, "old.stl"), old);
    scan(&f, &OsMetadataInspector);
    assert_eq!(count(&f, "SELECT count(*) FROM models"), 2);
    assert_eq!(model(&f, "hidden.stl"), hidden);
}
