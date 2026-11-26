mod common;

use common::{Events, Fixture};
use meshcatalog_core::{pipeline::run_scan, scanner::attr_guard::OsMetadataInspector};
use tokio_util::sync::CancellationToken;

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
