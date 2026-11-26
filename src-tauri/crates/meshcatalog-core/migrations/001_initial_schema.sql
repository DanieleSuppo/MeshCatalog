CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('local_dir', 'dropbox')),
    name         TEXT NOT NULL,
    root_uri     TEXT NOT NULL UNIQUE,
    cursor_token TEXT,
    status       TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'scanning', 'unavailable', 'error')),
    last_error   TEXT,
    last_scan_at INTEGER,
    created_at   INTEGER NOT NULL
);

CREATE TABLE folders (
    id                    INTEGER PRIMARY KEY,
    source_id             INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    parent_id             INTEGER REFERENCES folders(id) ON DELETE CASCADE,
    rel_path              TEXT NOT NULL,
    name                  TEXT NOT NULL,
    model_count           INTEGER NOT NULL DEFAULT 0,
    file_count            INTEGER NOT NULL DEFAULT 0,
    recursive_model_count INTEGER NOT NULL DEFAULT 0,
    recursive_file_count  INTEGER NOT NULL DEFAULT 0,
    nested_folder_count   INTEGER NOT NULL DEFAULT 0,
    sample_titles_json    TEXT,
    hero_thumb_id         INTEGER,
    preview_json          TEXT,
    summary_json          TEXT,
    UNIQUE(source_id, rel_path)
);

CREATE TABLE models (
    id                    INTEGER PRIMARY KEY,
    source_id             INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    folder_id             INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    container_kind        TEXT NOT NULL CHECK (container_kind IN ('folder_pack', 'zip_archive', 'single_file')),
    canonical_uri         TEXT NOT NULL UNIQUE,
    display_title         TEXT NOT NULL,
    raw_name              TEXT NOT NULL,
    fingerprint           TEXT,
    primary_file_id       INTEGER,
    primary_ext           TEXT,
    has_cloud_placeholder INTEGER NOT NULL DEFAULT 0,
    hero_thumb_id         INTEGER,
    file_count            INTEGER NOT NULL DEFAULT 0,
    total_bytes           INTEGER NOT NULL DEFAULT 0,
    updated_at            INTEGER NOT NULL
);

CREATE TABLE files (
    id                INTEGER PRIMARY KEY,
    model_id          INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    source_id         INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    folder_id         INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    parent_archive_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
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
    index_pass        INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX idx_folders_parent ON folders(source_id, parent_id);
CREATE INDEX idx_models_folder ON models(folder_id);
CREATE INDEX idx_models_source ON models(source_id);
CREATE INDEX idx_models_fingerprint ON models(fingerprint) WHERE fingerprint IS NOT NULL;
CREATE INDEX idx_files_hash ON files(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX idx_files_model ON files(model_id);
CREATE INDEX idx_files_folder ON files(folder_id);
CREATE INDEX idx_files_ext ON files(ext);
CREATE INDEX idx_files_parent_archive ON files(parent_archive_id) WHERE parent_archive_id IS NOT NULL;

CREATE TABLE thumbnails (
    id            INTEGER PRIMARY KEY,
    cache_key     TEXT NOT NULL UNIQUE,
    rel_webp_path TEXT NOT NULL,
    origin        TEXT NOT NULL CHECK (origin IN ('creator_image', '3mf_plate', 'lys_preview', 'wgpu_render', 'dropbox_thumb')),
    frame_count   INTEGER NOT NULL DEFAULT 1,
    width         INTEGER NOT NULL,
    height        INTEGER NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE tags (
    id       INTEGER PRIMARY KEY,
    facet    TEXT NOT NULL,
    slug     TEXT NOT NULL,
    label_it TEXT NOT NULL,
    label_en TEXT NOT NULL,
    UNIQUE(facet, slug)
);

CREATE TABLE model_tags (
    model_id      INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    tag_id        INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    origin        TEXT NOT NULL CHECK (origin IN ('auto', 'user_override')),
    confidence    REAL NOT NULL DEFAULT 1.0,
    is_excluded   INTEGER NOT NULL DEFAULT 0,
    evidence_json TEXT,
    PRIMARY KEY (model_id, tag_id)
);

CREATE INDEX idx_model_tags_tag ON model_tags(tag_id, is_excluded);

CREATE TABLE fingerprint_overrides (
    fingerprint TEXT NOT NULL,
    tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    is_excluded INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (fingerprint, tag_id)
);

-- Standalone FTS: the catalog writer updates these rows in the same transaction.
CREATE VIRTUAL TABLE models_fts USING fts5(
    model_id UNINDEXED,
    title,
    path_tokens,
    file_tokens,
    tags_it_en,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE files_fts USING fts5(
    file_id UNINDEXED,
    model_id UNINDEXED,
    filename,
    rel_path,
    ext,
    tokenize = 'unicode61 remove_diacritics 2'
);
