use meshcatalog_core::db::Database;
use rusqlite::{params, Connection, ErrorCode};

#[test]
fn initial_schema_enforces_constraints_and_supports_accent_insensitive_search() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    db.execute_batch(include_str!("../migrations/001_initial_schema.sql"))
        .unwrap();
    db.execute_batch(
        "INSERT INTO sources (id, kind, name, root_uri, created_at) VALUES (1, 'local_dir', 'Test', 'local://D:/Test', 1);
         INSERT INTO folders (id, source_id, name, rel_path) VALUES (1, 1, 'Test', '');
         INSERT INTO models (id, source_id, folder_id, container_kind, canonical_uri, display_title, raw_name, updated_at)
             VALUES (1, 1, 1, 'single_file', 'local://D:/Test/model.stl', 'Crème', 'Crème.stl', 1);
         INSERT INTO files (id, model_id, source_id, folder_id, uri, rel_path, filename, ext, storage_state, size_bytes)
             VALUES (1, 1, 1, 1, 'local://D:/Test/model.stl', 'Crème.stl', 'Crème.stl', 'stl', 'local_resident', 7);
         INSERT INTO models_fts (model_id, title) VALUES (1, 'Crème');
         INSERT INTO files_fts (file_id, model_id, filename) VALUES (1, 1, 'Crème.stl');",
    ).unwrap();

    for query in [
        "SELECT count(*) FROM models_fts WHERE models_fts MATCH 'creme'",
        "SELECT count(*) FROM files_fts WHERE files_fts MATCH 'creme'",
    ] {
        assert_eq!(
            db.query_row(query, [], |row| row.get::<_, i64>(0)).unwrap(),
            1
        );
    }
    for sql in [
        "UPDATE sources SET kind = 'remote'",
        "UPDATE sources SET status = 'done'",
        "INSERT INTO sources (kind, name, root_uri, created_at) SELECT kind, name, root_uri, created_at FROM sources",
        "INSERT INTO folders (source_id, name, rel_path) SELECT source_id, name, rel_path FROM folders",
        "UPDATE models SET container_kind = 'archive'",
        "INSERT INTO models (source_id, container_kind, canonical_uri, display_title, raw_name, updated_at) SELECT source_id, container_kind, canonical_uri, display_title, raw_name, updated_at FROM models",
        "UPDATE files SET storage_state = 'remote'",
        "INSERT INTO files (model_id, source_id, uri, rel_path, filename, ext, storage_state, size_bytes) SELECT model_id, source_id, uri, rel_path, filename, ext, storage_state, size_bytes FROM files",
    ] {
        assert_eq!(db.execute(sql, []).unwrap_err().sqlite_error_code(), Some(ErrorCode::ConstraintViolation), "{sql}");
    }

    db.execute("DELETE FROM sources WHERE id = 1", []).unwrap();
    for query in [
        "SELECT count(*) FROM folders",
        "SELECT count(*) FROM models",
        "SELECT count(*) FROM files",
    ] {
        assert_eq!(
            db.query_row(query, [], |row| row.get::<_, i64>(0)).unwrap(),
            0
        );
    }
}

#[test]
fn database_reopens_without_losing_data_and_readers_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    let db = Database::open(&path).unwrap();
    db.with_writer(|writer| {
        assert_eq!(writer.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))?, "wal");
        for (pragma, value) in [("foreign_keys", 1), ("synchronous", 1), ("busy_timeout", 5000), ("temp_store", 2), ("user_version", 2)] {
            assert_eq!(writer.pragma_query_value(None, pragma, |r| r.get::<_, i64>(0))?, value);
        }
        let tx = writer.transaction()?;
        tx.execute("INSERT INTO sources (kind, name, root_uri, created_at) VALUES ('local_dir', 'Test', ?1, 1)", params!["local://D:/Test"])?;
        // A reader must not wait on the writer lock or observe uncommitted rows.
        db.with_reader(|reader| {
            assert_eq!(reader.query_row("SELECT count(*) FROM sources", [], |r| r.get::<_, i64>(0))?, 0);
            let error = reader.execute("DELETE FROM sources", []).unwrap_err();
            assert_eq!(error.sqlite_error_code(), Some(ErrorCode::ReadOnly));
            for (pragma, value) in [("foreign_keys", 1), ("synchronous", 1), ("busy_timeout", 5000), ("temp_store", 2)] {
                assert_eq!(reader.pragma_query_value(None, pragma, |r| r.get::<_, i64>(0))?, value);
            }
            Ok(())
        })?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    db.with_reader(|reader| {
        assert_eq!(
            reader.query_row("SELECT status FROM sources", [], |r| r.get::<_, String>(0))?,
            "idle"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn upgrades_v1_preserving_archive_relationships_identities_fts_and_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    let v1 = Connection::open(&path).unwrap();
    v1.execute_batch(include_str!("../migrations/001_initial_schema.sql"))
        .unwrap();
    v1.execute_batch("PRAGMA user_version=1;
        INSERT INTO sources (id, kind, name, root_uri, created_at) VALUES (1,'local_dir','Test','local://test',0);
        INSERT INTO folders (id, source_id, rel_path, name) VALUES (7,1,'','Test');
        INSERT INTO models (id, source_id, folder_id, container_kind, canonical_uri, display_title, raw_name, fingerprint, primary_file_id, updated_at)
          VALUES (11,1,7,'zip_archive','local://test/a.zip','Archive','a.zip','fingerprint',21,0);
        INSERT INTO files (id, model_id, source_id, folder_id, uri, rel_path, filename, ext, storage_state, size_bytes)
          VALUES (21,11,1,7,'local://test/a.zip','a.zip','a.zip','zip','local_resident',60),
                 (22,11,1,7,'zip://local://test/a.zip#mesh.stl','mesh.stl','mesh.stl','stl','zip_entry',8);
        UPDATE files SET parent_archive_id=21, zip_entry_method=8, zip_entry_csize=4 WHERE id=22;
        INSERT INTO files_fts (rowid,file_id,model_id,filename) VALUES (22,22,11,'mesh');
        INSERT INTO models_fts (rowid,model_id,title) VALUES (11,11,'Archive');
        INSERT INTO tags VALUES (1,'kind','test','Test','Test');
        INSERT INTO model_tags (model_id,tag_id,origin) VALUES (11,1,'user_override');
        INSERT INTO fingerprint_overrides (fingerprint,tag_id,updated_at) VALUES ('fingerprint',1,0);").unwrap();
    drop(v1);
    let db = Database::open(&path).unwrap();
    db.with_writer(|c| {
        assert_eq!(c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get::<_, i64>(0))?, 0);
        for sql in [
            "SELECT count(*) FROM files WHERE id=22 AND parent_archive_id=21 AND model_id=11 AND folder_id=7 AND zip_entry_method=8 AND zip_entry_csize=4",
            "SELECT count(*) FROM files_fts WHERE files_fts MATCH 'mesh' AND rowid=22",
            "SELECT count(*) FROM models_fts WHERE models_fts MATCH 'archive' AND rowid=11",
            "SELECT count(*) FROM model_tags WHERE model_id=11 AND origin='user_override'",
            "SELECT count(*) FROM fingerprint_overrides WHERE fingerprint='fingerprint'",
            "SELECT count(*) FROM pragma_index_list('files') WHERE name='idx_files_parent_archive'",
        ] {
            assert_eq!(c.query_row(sql, [], |r|r.get::<_,i64>(0))?, 1, "{sql}");
        }
        c.execute("INSERT INTO files (source_id,folder_id,uri,rel_path,filename,ext,storage_state,size_bytes) VALUES (1,7,'local://test/cover.png','cover.png','cover.png','png','local_resident',8)", [])?;
        assert!(c.execute("UPDATE files SET fileName='solo.stl', ext='stl' WHERE model_id IS NULL", []).is_err());
        c.execute("DELETE FROM files WHERE id=21", [])?;
        assert_eq!(c.query_row("SELECT count(*) FROM files WHERE id=22", [], |r|r.get::<_,i64>(0))?, 0);
        Ok(())
    }).unwrap();
    drop(db);
    Database::open(&path).unwrap();
}

#[test]
fn database_rejects_future_schema_without_changing_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(Database::open(&path).is_err());
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
}
