-- No other table declares an FK to files. Copy IDs and the self-reference before replacing it.
CREATE TABLE files_v2 (
    id                INTEGER PRIMARY KEY,
    model_id          INTEGER REFERENCES models(id) ON DELETE CASCADE,
    source_id         INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    folder_id         INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    parent_archive_id INTEGER REFERENCES files_v2(id) ON DELETE CASCADE,
    uri               TEXT NOT NULL UNIQUE,
    rel_path          TEXT NOT NULL,
    filename          TEXT NOT NULL,
    ext               TEXT NOT NULL,
    storage_state     TEXT NOT NULL CHECK (storage_state IN ('local_resident', 'cloud_placeholder', 'dropbox_remote', 'zip_entry')),
    size_bytes        INTEGER NOT NULL,
    mtime_unix        INTEGER,
    content_hash      TEXT,
    zip_entry_offset  INTEGER,
    zip_entry_csize   INTEGER,
    zip_entry_method  INTEGER,
    triangle_count    INTEGER,
    bbox_x_mm         REAL,
    bbox_y_mm         REAL,
    bbox_z_mm         REAL,
    thumb_id          INTEGER,
    index_pass        INTEGER NOT NULL DEFAULT 1,
    CHECK (model_id IS NOT NULL OR (
        ext IN ('jpg', 'jpeg', 'png', 'webp') AND parent_archive_id IS NULL AND storage_state <> 'zip_entry'
    ))
);

INSERT INTO files_v2
SELECT id, model_id, source_id, folder_id, parent_archive_id, uri, rel_path, filename, ext,
       storage_state, size_bytes, mtime_unix, content_hash, zip_entry_offset, zip_entry_csize,
       zip_entry_method, triangle_count, bbox_x_mm, bbox_y_mm, bbox_z_mm, thumb_id, index_pass
FROM files;

DROP TABLE files;
ALTER TABLE files_v2 RENAME TO files;
CREATE INDEX idx_files_hash ON files(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX idx_files_model ON files(model_id);
CREATE INDEX idx_files_folder ON files(folder_id);
CREATE INDEX idx_files_ext ON files(ext);
CREATE INDEX idx_files_parent_archive ON files(parent_archive_id) WHERE parent_archive_id IS NOT NULL;
CREATE INDEX idx_files_source ON files(source_id);
