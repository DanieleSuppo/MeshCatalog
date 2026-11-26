mod common;

use common::{Events, Fixture};
use meshcatalog_core::{
    contracts::{
        IndexerBatchUpdatedEvent, IndexerErrorEvent, IndexerProgressEvent, SearchQuery,
        SourceFilter, ViewMode,
    },
    db::Database,
    pipeline::{run_pass1, run_scan, PipelineSink},
    scanner::attr_guard::OsMetadataInspector,
};
use tokio_util::sync::CancellationToken;

#[test]
fn root_keeps_technical_children_even_with_loose_files() {
    let f = Fixture::new();
    f.file("STL/root.stl", b"a");
    for path in ["README.md", "loose.stl"] {
        f.file(path, b"b");
        run_scan(
            &f.db,
            f.source,
            &OsMetadataInspector,
            &CancellationToken::new(),
            &mut Events::default(),
        )
        .unwrap();
        let tree = f.db.get_folder_tree(SourceFilter::All).unwrap();
        assert_eq!(tree[0].children[0].rel_path, "STL", "with {path}");
        f.db.with_reader(|db| {
            let title: String = db.query_row("SELECT display_title FROM models m JOIN folders f ON f.id=m.folder_id WHERE f.rel_path='STL'", [], |r| r.get(0))?;
            assert_eq!(title, "root");
            Ok(())
        }).unwrap();
    }
}

#[test]
fn only_a_leaf_with_an_indirect_single_member_takes_the_folder_name() {
    let f = Fixture::new();
    for path in [
        "Boss_Krull/STL/body.stl",
        "Direct/body.stl",
        "Mixed/STL/body.stl",
        "Mixed/Other/part.stl",
        "Zipped/STL/body.stl",
        "Zipped/bonus.zip",
    ] {
        f.file(path, b"x");
    }
    run_scan(
        &f.db,
        f.source,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    let tree = f.db.get_folder_tree(SourceFilter::All).unwrap();
    assert_eq!(
        tree[0]
            .children
            .iter()
            .map(|f| f.rel_path.as_str())
            .collect::<Vec<_>>(),
        ["Direct", "Mixed", "Zipped"]
    );
    f.db.with_reader(|db| {
        let mut stmt = db.prepare("SELECT m.display_title,m.container_kind,f.rel_path FROM models m JOIN folders f ON f.id=m.folder_id ORDER BY 1,3")?;
        let models = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
        assert_eq!(models, [("Boss Krull", "folder_pack", ""), ("body", "single_file", "Direct"), ("body", "single_file", "Mixed"), ("body", "single_file", "Zipped"), ("bonus", "zip_archive", "Zipped"), ("part", "single_file", "Mixed/Other")].map(|(title,kind,folder)|(title.to_owned(),kind.to_owned(),folder.to_owned())));
        Ok(())
    }).unwrap();
}

#[test]
fn completed_pass1_drops_empty_provisional_models() {
    let f = Fixture::new();
    f.file("Boss_Krull/STL/body.stl", b"mesh");
    run_pass1(
        &f.db,
        f.source,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    assert_eq!(f.db.list_sources().unwrap()[0].model_count, 1);
}

#[test]
fn mid_inventory_batches_keep_technical_files_searchable_without_provisional_folders() {
    struct Observe<'a> {
        db: &'a Database,
        batches: usize,
        first_scan: bool,
    }
    impl PipelineSink for Observe<'_> {
        fn progress(&mut self, _: IndexerProgressEvent) {}
        fn error(&mut self, e: IndexerErrorEvent) {
            panic!("{}", e.message);
        }
        fn batch_updated(&mut self, _: IndexerBatchUpdatedEvent) {
            self.batches += 1;
            let query = |text: &str, folder_id| SearchQuery {
                text: text.into(),
                view_mode: ViewMode::Files,
                source_filter: SourceFilter::All,
                source_id: None,
                folder_id,
                facets: Default::default(),
                limit: 10,
                offset: 0,
            };
            let tree = self.db.get_folder_tree(SourceFilter::All).unwrap();
            fn contains(nodes: &[meshcatalog_core::contracts::FolderTreeNode], path: &str) -> bool {
                nodes
                    .iter()
                    .any(|f| f.rel_path == path || contains(&f.children, path))
            }
            assert!(!contains(&tree, "Pack/STL"));
            if let Some(pack) = tree[0].children.iter().find(|dir| dir.rel_path == "Pack") {
                let browse = self.db.search_catalog(&query("", Some(pack.id))).unwrap();
                assert!(browse.subfolders.iter().all(|dir| dir.name != "STL"));
            }
            let page = self
                .db
                .search_catalog(&query("Mesh", Some(tree[0].id)))
                .unwrap();
            assert!(page.total_files > 0);
            if self.first_scan && self.batches == 1 {
                assert!(page.total_files < 510);
            }
            assert!(page.items.iter().all(|item| matches!(item,
                meshcatalog_core::contracts::SearchItemDto::File(file)
                    if file.rel_path.starts_with("Pack/STL/Mesh"))));
            if self
                .db
                .search_catalog(&query("zip", None))
                .unwrap()
                .total_files
                > 0
            {
                assert!(contains(&tree, "Misc/Images"));
            }
        }
    }
    let f = Fixture::new();
    for i in 0..510 {
        f.file(&format!("Pack/STL/Mesh{i}.stl"), b"a");
    }
    // A valid, empty ZIP makes Images non-absorbable without a metadata error.
    f.file(
        "Misc/Images/separate.zip",
        b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    );
    f.file("Lonely/Images/cover.png", b"image without a model");
    for first_scan in [true, false] {
        let mut sink = Observe {
            db: &f.db,
            batches: 0,
            first_scan,
        };
        run_scan(
            &f.db,
            f.source,
            &OsMetadataInspector,
            &CancellationToken::new(),
            &mut sink,
        )
        .unwrap();
        assert!(sink.batches > 2);
        let tree = f.db.get_folder_tree(SourceFilter::All).unwrap();
        let lonely = tree[0]
            .children
            .iter()
            .find(|folder| folder.rel_path == "Lonely")
            .unwrap();
        assert_eq!(lonely.children[0].rel_path, "Lonely/Images");
    }
}

#[test]
fn title_cleanup_removes_decimal_scales_and_ratios_without_changing_hyphenated_names() {
    let f = Fixture::new();
    f.file(
        "2024_07_[Creator]_King-Thror_10.5mm_1_10_pre-supported_v3.STL",
        b"fixture",
    );
    run_scan(
        &f.db,
        f.source,
        &OsMetadataInspector,
        &CancellationToken::new(),
        &mut Events::default(),
    )
    .unwrap();
    f.db.with_reader(|db| {
        let title: String = db.query_row("SELECT display_title FROM models", [], |r| r.get(0))?;
        assert_eq!(title, "King-Thror");
        Ok(())
    })
    .unwrap();
}

#[test]
fn grouped_packs_fold_only_terminal_technical_folders_and_keep_context() {
    let f = Fixture::new();
    for dir in [
        "STL",
        "stls",
        "Presupported",
        "pre-supported",
        "supported",
        "unsupported",
        "images",
        "renders",
        "render",
        "lychee",
        "files",
        "32mm",
        "75mm",
        "10,5 mm",
        "1_10",
        "1-12",
    ] {
        f.file(
            &format!("2025_03_[Creator]_Dragon.Pack_v2/{dir}/body.stl"),
            b"a",
        );
    }
    for path in [
        "STL/root.stl",
        "Mixed/part.stl",
        "Mixed/photo.jpg",
        "Mixed/Images/inside.zip",
        "Mixed/Files/Sub/other.obj",
        "Mixed/Else/solo.3mf",
        "Pictures/cover.webp",
        "[Creator]_32mm_v2.stl/part.stl",
        "[Creator]_32mm_v2.stl/photo.png",
        "__MACOSX/no.stl",
        "._ignore.stl",
    ] {
        f.file(path, b"x");
    }
    for _ in 0..2 {
        run_scan(
            &f.db,
            f.source,
            &OsMetadataInspector,
            &CancellationToken::new(),
            &mut Events::default(),
        )
        .unwrap();
        f.db.with_reader(|db| {
            let mut stmt = db.prepare("SELECT raw_name, display_title, container_kind, rel_path, m.file_count FROM models m JOIN folders f ON m.folder_id=f.id ORDER BY raw_name")?;
            let actual = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,i64>(4)?)))?.collect::<Result<Vec<_>, _>>()?;
            let expected = [
                ("2025_03_[Creator]_Dragon.Pack_v2", "Dragon Pack", "folder_pack", "", 16),
                ("Mixed", "Mixed", "folder_pack", "Mixed", 2),
                ("[Creator]_32mm_v2.stl", "[Creator]_32mm_v2.stl", "folder_pack", "", 2),
                ("inside.zip", "inside", "zip_archive", "Mixed/Images", 1),
                ("other.obj", "other", "single_file", "Mixed/Files/Sub", 1),
                ("root.stl", "root", "single_file", "STL", 1),
                ("solo.3mf", "solo", "single_file", "Mixed/Else", 1),
            ].map(|(a,b,c,d,e)| (a.to_string(), b.to_string(), c.to_string(), d.to_string(), e));
            assert_eq!(actual, expected);
            let counts = db.query_row("SELECT recursive_model_count, recursive_file_count, nested_folder_count, json_array_length(sample_titles_json), json_extract(summary_json,'$.cloud') FROM folders WHERE parent_id IS NULL", [], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?)))?;
            assert_eq!(counts, (7, 25, 7, 4, 0));
            assert_eq!(db.query_row("SELECT count(*) FROM files JOIN folders ON files.folder_id=folders.id WHERE files.model_id IS NULL AND files.filename='cover.webp' AND folders.rel_path='Pictures'", [], |r| r.get::<_, i64>(0))?, 1);
            Ok(())
        }).unwrap();
    }
}
