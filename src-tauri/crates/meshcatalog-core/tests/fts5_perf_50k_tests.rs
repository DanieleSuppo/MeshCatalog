mod common;

use common::{Events, Fixture};
use meshcatalog_core::{
    contracts::{SearchItemDto, SearchQuery, SourceFilter, ViewMode},
    db::Database,
    error::IpcError,
    pipeline::run_scan,
    scanner::attr_guard::OsMetadataInspector,
};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

fn query(text: &str, view_mode: ViewMode, folder_id: Option<i64>) -> SearchQuery {
    SearchQuery {
        text: text.to_owned(),
        view_mode,
        source_filter: SourceFilter::All,
        source_id: None,
        folder_id,
        facets: Default::default(),
        limit: 20,
        offset: 0,
    }
}

#[test]
fn global_and_recursive_fts_50k_respect_budget_and_literal_punctuation() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("catalog.db")).unwrap();
    db.with_writer(|db| {
        let tx = db.transaction()?;
        tx.execute_batch("INSERT INTO sources (id,kind,name,root_uri,created_at) VALUES (1,'local_dir','Library','local://library',0),(2,'dropbox','Other','dropbox://other',0);
            INSERT INTO folders (id,source_id,parent_id,rel_path,name) VALUES (1,1,NULL,'','Library'),(2,1,1,'Collection','Collection'),(3,1,2,'Collection/Deep','Deep'),(4,2,NULL,'','Other');
            WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<50010)
            INSERT INTO models (id,source_id,folder_id,container_kind,canonical_uri,display_title,raw_name,primary_ext,file_count,total_bytes,updated_at)
            SELECT x, CASE WHEN x=50010 THEN 2 ELSE 1 END, CASE WHEN x<=50000 THEN 3 WHEN x<50010 THEN 1 ELSE 4 END, 'single_file',
            CASE WHEN x<=50000 THEN 'local://library/Collection/Deep/' WHEN x<50010 THEN 'local://library/' ELSE 'dropbox://other/' END || x || '.stl',
            CASE WHEN x IN (1,50001,50010) THEN 'King-Thror' ELSE 'Common sculpture' END, 'Asset ' || x || '.stl', 'stl', 1, 7, 0 FROM n;
            INSERT INTO files (id,model_id,source_id,folder_id,uri,rel_path,filename,ext,storage_state,size_bytes)
              SELECT id,id,source_id,folder_id,canonical_uri,raw_name,raw_name,'stl','local_resident',7 FROM models;
            INSERT INTO models_fts (rowid,model_id,title,path_tokens,file_tokens) SELECT id,id,display_title,canonical_uri,raw_name FROM models;
            INSERT INTO files_fts (rowid,file_id,model_id,filename,rel_path,ext)
              SELECT f.id,f.id,f.model_id,m.display_title || ' ' || f.filename,f.uri,f.ext FROM files f JOIN models m ON m.id=f.model_id;")?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    for view in [ViewMode::Models, ViewMode::Files] {
        for (folder, all, k) in [(None, 50010, 3), (Some(2), 50000, 1)] {
            for (text, count) in [
                ("King-Thror", k),
                ("\"King-Thror\"*", k),
                ("\"Kin-Thr\"*", k),
                ("Kin* OR Thr", 0),
                (".st", all),
                (".stl", all),
                ("\".*-", 0),
            ] {
                let start = Instant::now();
                let page = db.search_catalog(&query(text, view, folder)).unwrap();
                let time = start.elapsed();
                eprintln!(
                    "{view:?} {folder:?} {text:?}: {:.2} ms",
                    time.as_secs_f64() * 1000.0
                );
                assert!(
                    time < Duration::from_millis(100),
                    "{view:?} {folder:?} {text:?}: {time:?}"
                );
                assert_eq!(
                    (page.total_models, page.total_files, page.items.len()),
                    (count, count, count.min(20) as usize)
                );
                assert!(page.subfolders.is_empty());
            }
        }
    }
    let mut q = query("King-Thror", ViewMode::Files, None);
    q.source_filter = SourceFilter::Local;
    q.offset = 1;
    q.limit = 1;
    let page = db.search_catalog(&q).unwrap();
    assert_eq!(
        (page.total_models, page.total_files, page.items.len()),
        (2, 2, 1)
    );
    let SearchItemDto::File(file) = &page.items[0] else {
        panic!("wrong view");
    };
    assert_eq!(file.source_id, 1);
    q.source_id = Some(2);
    assert!(db.search_catalog(&q).unwrap().items.is_empty());
}

#[test]
fn root_mixed_browse_and_detail_include_orphan_images_only_as_files() {
    let f = Fixture::new();
    for path in [
        "Collection/direct.stl",
        "Collection/Pack/STL/b.stl",
        "Collection/Pack/STL/a.3mf",
        "Collection/Pictures/CoverOnly.png",
        "Collection/Pictures/Nested/Another.jpg",
    ] {
        f.file(path, b"fixture");
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
        let roots =
            f.db.search_catalog(&query(" ", ViewMode::Models, None))
                .unwrap();
        assert!(roots.items.is_empty());
        assert_eq!(
            (
                roots.total_models,
                roots.total_files,
                roots.subfolders.len()
            ),
            (2, 5, 1)
        );
        assert_eq!(
            (
                roots.subfolders[0].name.as_str(),
                roots.subfolders[0].sample_titles.as_slice()
            ),
            (
                "Test library",
                &["Pack".to_owned(), "direct".to_owned()][..]
            )
        );
        let root_id = roots.subfolders[0].id;
        let nodes = f.db.get_folder_tree(SourceFilter::Local).unwrap();
        assert_eq!(nodes[0].id, root_id);
        let collection = &nodes[0].children[0];
        assert_eq!(collection.children.len(), 1);
        assert_eq!(collection.children[0].rel_path, "Collection/Pictures");
        let page =
            f.db.search_catalog(&query("", ViewMode::Models, Some(collection.id)))
                .unwrap();
        assert_eq!(
            (page.total_models, page.total_files, page.subfolders.len()),
            (2, 3, 1)
        );
        assert_eq!(
            (
                page.subfolders[0].recursive_model_count,
                page.subfolders[0].recursive_file_count,
                page.subfolders[0].nested_folder_count
            ),
            (0, 2, 1)
        );
        let pack = page
            .items
            .iter()
            .find_map(|i| match i {
                SearchItemDto::Model(m) if m.display_title == "Pack" => Some(m),
                _ => None,
            })
            .unwrap();
        assert_eq!(pack.relative_path, "Collection/Pack");
        let detail = f.db.get_model_detail(pack.id).unwrap();
        assert_eq!(
            detail
                .files
                .iter()
                .map(|f| f.rel_path.as_str())
                .collect::<Vec<_>>(),
            ["STL/a.3mf", "STL/b.stl"]
        );
        let scoped =
            f.db.search_catalog(&query("CoverOnly", ViewMode::Files, Some(collection.id)))
                .unwrap();
        assert_eq!((scoped.total_models, scoped.total_files), (0, 1));
        let SearchItemDto::File(image) = &scoped.items[0] else {
            panic!("wrong view");
        };
        assert_eq!(image.model_id, None);
        assert_eq!(image.rel_path, "Collection/Pictures/CoverOnly.png");
        assert!(f
            .db
            .search_catalog(&query("CoverOnly", ViewMode::Models, Some(root_id)))
            .unwrap()
            .items
            .is_empty());
        assert!(f
            .db
            .search_catalog(&query("CoverOnly", ViewMode::Files, Some(-1)))
            .unwrap()
            .items
            .is_empty());
        assert!(matches!(
            f.db.get_model_detail(-1),
            Err(IpcError::ModelNotFound)
        ));
    }
    f.db.remove_source(f.source).unwrap();
    assert!(f.root.join("Collection/Pictures/CoverOnly.png").is_file());
    assert_eq!(
        f.db.search_catalog(&query("CoverOnly", ViewMode::Files, None))
            .unwrap()
            .total_files,
        0
    );
}
