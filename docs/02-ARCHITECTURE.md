# 02 — Architettura Tecnica e Schema Dati (`MeshCatalog`)

## 1. Stack Tecnologico

| Componente | Scelta Tecnica | Motivazione |
| :--- | :--- | :--- |
| **Framework Desktop** | **Tauri v2** | Usa Edge WebView2 nativo su Windows (~10 MB installer, basso consumo RAM) con backend nativo Rust. |
| **Backend Core** | **Rust** (Edition 2021/2024) | Elaborazione parallela multi-core (`rayon` + `tokio`), parsing binario STL/ZIP velocissimo, accesso diretto Win32 API. |
| **Database & Search** | **SQLite** con estensione **`FTS5`** (`rusqlite` `bundled`) | Zero installazione esterna, query < 5 ms su 100k+ record, transazioni ACID, modalità `WAL` per letture UI non bloccanti durante l'indicizzazione. |
| **Hashing** | **`blake3`** | Hashing parallelo SIMD multi-GB/s per file locali; abbinato a `content_hash` nativo di Dropbox. |
| **Archivi & HTTP** | **`zip` / `async_zip`** + **`reqwest`** | Lettura streaming/range della Central Directory ZIP senza estrarre file su disco. |
| **Thumbnailing 3D** | **`wgpu`** (headless off-screen renderer) | Generazione batch su GPU (DX12/Vulkan su Windows) delle miniature `.webp` multi-angolo in background senza aprire finestre. |
| **Frontend UI** | **React 19 + TypeScript + Vite** | Ecosistema maturo; virtualizzazione griglia con `@tanstack/react-virtual` per scorrimento fluido a 60+ fps. |
| **Viewer 3D On-Demand** | **Three.js** (`@react-three/fiber` + `@react-three/drei`) | Attivato esclusivamente nell'Inspector / QuickLook al click dell'utente. |

---

## 2. Architettura "Zero-Explosion" per Dropbox (~660 GB)

Su Windows, Dropbox in modalità *Smart Sync (Online-Only)* espone i file cloud nel filesystem come *placeholder NTFS*. Se un processo li apre con `File::open`, Windows intercetta la chiamata e **forza il download completo del file su SSD**.

### 2.1 Scudo Anti-Idratazione su Disco Locale (Win32 Guard)
Prima di enumerare una directory o aprire un file, lo scanner verifica i metadati Win32, partendo dalla radice del volume e controllando gli antenati. `FindFirstFileW` restituisce attributi e tag reparse senza aprire un handle di lettura dati. Per la radice del volume si usa `GetFileAttributesW`; se manca il tag di una radice reparse, lo scanner la rifiuta.

- `FILE_ATTRIBUTE_OFFLINE` (`0x0000_1000`), `FILE_ATTRIBUTE_RECALL_ON_OPEN` (`0x0004_0000`) e `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (`0x0040_0000`) bloccano le aperture dati, l'enumerazione delle cartelle e le anteprime Shell. I segnaposto vengono catalogati con i soli metadati.
- Tra i reparse point, solo i tag riconosciuti della famiglia `IO_REPARSE_TAG_CLOUD` senza bit di richiamo/offline sono trattati come residenti. Junction, symlink e altri name surrogate non vengono seguiti; un tag mancante/sconosciuto o un errore di lookup non autorizza l'accesso.

### 2.2 Connettore Diretto Dropbox API v2 (OAuth 2.0 PKCE Locale)
L'app integra un flusso OAuth 2.0 PKCE nativo desktop (nessun server intermedio; refresh token salvato in modo sicuro nel credential store Windows / config locale):

1. **Scansione Albero (`POST /2/files/list_folder` + `/list_folder/continue`):**
   - Scarica solo metadati JSON (path, nome, dimensione in byte, `content_hash`, `rev`, timestamp).
   - Aggiornamenti incrementali successivi tramite `cursor` persistito in SQLite.
   - Consumo disco per la cartella Dropbox: **0 byte**.
2. **Ispezione Chirurgica `.zip` e `.3mf` Remoti via `HTTP Range Requests`:**
   - Per ottenere un URL diretto temporaneo si usa `/2/files/get_temporary_link` (oppure `/2/files/download` con header `Range: bytes=...`).
   - **Step A (Elenco contenuti):** Richiesta parziale degli **ultimi 64 KB** del file (`HTTP 206 Partial Content`) per trovare il record *End of Central Directory (EOCD)*; se la *Central Directory* si trova a un offset precedente, una seconda richiesta `Range` mirata ne scarica esattamente i soli byte necessari.
     - Risultato: conosciamo tutti gli `.stl`, `.3mf`, `.lys`, `.jpg`/`.png` dentro uno `.zip` da 3 GB scaricando ~64-300 KB.
   - **Step B (Estrazione copertine):** Se nella Central Directory dell'archivio sono presenti immagini render (`.jpg`/`.png`/`.webp`) o `Metadata/plate_*.png` (nei `.3mf`) o `preview.png` (nei `.lys`), una richiesta `Range` mirata sul singolo `local_header_offset + compressed_size` scarica **solo quell'immagine in RAM**, la converte in `.webp` nella cache locale e libera la memoria.
3. **Interruttore Utente per Soli-STL su Cloud:**
   - Impostazione esplicita: **`render_cloud_stl_in_ram: bool`** (default: `false` o attivabile durante l'indicizzazione notturna).
   - Quando attiva, per i modelli su Dropbox che non hanno nessuna immagine di copertina né `.3mf`/`.lys` con preview, l'app scarica in **RAM volatile** *un solo* `.stl` rappresentativo (es. il più grande non-supporto/non-basetta), renderizza la thumbnail `.webp` multi-angolo con `wgpu` nella cache locale e scarta immediatamente il buffer RAM senza scrivere mai il file `.stl` su SSD.

---

## 3. Pipeline di Indicizzazione a 3 Passate (Background Non-Bloccante)

Per privilegiare la reattività della UI anche quando l'indicizzazione completa richiede tempo:

1. **Passata 1 — Albero & Inventario Veloce (Secondi):**
   - Scansiona SSD locale (rispettando il guard Win32) + sync metadati Dropbox (`list_folder`).
   - Raggruppa provvisoriamente i modelli/cartelle e popola subito `folders`, `models`, `files` e `models_fts`.
   - **L'utente può già navigare e cercare per nome/path.**
2. **Passata 2 — Archivi, Hash, Preview Nativa & Tassonomia:**
   - Legge la Central Directory degli `.zip` (locali e remoti via `Range`).
   - Calcola `BLAKE3` per i file locali residenti (saltando i file con dimensione e `mtime` invariati rispetto al DB).
   - Riconosce eventuali file/modelli spostati confrontando `blake3_hash` / `dropbox_content_hash` e riaggancia i `user_override`.
   - Estrae miniature già pronte: immagini creator nei folder/zip, `Metadata/plate_*.png` dai `.3mf`, `preview.png` dai `.lys` (e `get_thumbnail_batch` Dropbox per immagini sfuse).
   - Esegue il **Motore Tassonomico Automatico** assegnando facce e confidenze.
3. **Passata 3 — Macinazione Geometria & Render Headless (`wgpu`):**
   - Analizza mesh `.stl`/`.obj`/`.3mf` locali (e quelli remoti in RAM solo se l'interruttore utente è attivo) per calcolare `triangle_count` e `bbox_x/y/z_mm`.
   - Renderizza una striscia/atlante `.webp` a 4 angolazioni (es. `0°`, `90°`, `180°`, `270°` oppure prospettica + viste ortogonali) per consentire l'*Hover Scrub* istantaneo nella UI senza WebGL.

---

## 4. Strategia Cache su Disco (`%LOCALAPPDATA%\MeshCatalog`)

```text
%LOCALAPPDATA%\MeshCatalog\
├── catalog.db                  # Database SQLite principale (WAL mode)
├── catalog.db-wal
├── config.json                 # Preferenze utente non sensibili
└── cache\
    └── thumbs\
        └── <ab>\<hash>.webp    # Thumbnail singole o sprite 4-angoli (max ~40-80 KB/cad)
```

---

## 5. Schema SQLite di Riferimento (`DDL`)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE sources (
    id              INTEGER PRIMARY KEY,
    kind            TEXT NOT NULL CHECK (kind IN ('local_dir', 'dropbox')),
    name            TEXT NOT NULL,
    root_uri        TEXT NOT NULL UNIQUE, -- es. "D:\Stampa3D" o "dropbox://root"
    cursor_token    TEXT,                 -- Dropbox list_folder cursor
    last_scan_at    INTEGER
);

CREATE TABLE folders (
    id              INTEGER PRIMARY KEY,
    source_id       INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    parent_id       INTEGER REFERENCES folders(id) ON DELETE CASCADE,
    rel_path        TEXT NOT NULL,        -- path relativo normalizzato con '/'
    name            TEXT NOT NULL,
    is_visible      INTEGER NOT NULL DEFAULT 1 CHECK (is_visible IN (0, 1)),
    model_count     INTEGER NOT NULL DEFAULT 0,
    file_count      INTEGER NOT NULL DEFAULT 0,
    hero_thumb_id   INTEGER,
    preview_json    TEXT,                 -- JSON array max 4 thumb_id per mosaico Smart Folder
    summary_json    TEXT,                 -- es. {"stl":42,"zip":3,"resin":12,"fdm":2}
    UNIQUE(source_id, rel_path)
);

CREATE TABLE models (
    id              INTEGER PRIMARY KEY,
    source_id       INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    folder_id       INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    container_kind  TEXT NOT NULL CHECK (container_kind IN ('folder_pack', 'zip_archive', 'single_file')),
    canonical_uri   TEXT NOT NULL UNIQUE, -- es. "local://D:/.../Drago" oppure "dropbox:///.../pack.zip"
    display_title   TEXT NOT NULL,        -- Titolo pulito dedotto da path/nome
    raw_name        TEXT NOT NULL,
    fingerprint     TEXT,                 -- Hash aggregato dei file contenuti (stabile su spostamenti)
    primary_file_id INTEGER,
    hero_thumb_id   INTEGER,
    file_count      INTEGER NOT NULL DEFAULT 0,
    total_bytes     INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE files (
    id               INTEGER PRIMARY KEY,
    model_id         INTEGER REFERENCES models(id) ON DELETE CASCADE, -- NULL solo per immagini non associate
    source_id        INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    folder_id        INTEGER REFERENCES folders(id) ON DELETE SET NULL,
    parent_archive_id INTEGER REFERENCES files(id) ON DELETE CASCADE, -- valorizzato se dentro uno .zip
    uri              TEXT NOT NULL UNIQUE, -- "local://...", "dropbox://...", "zip://<parent_uri>#<entry>"
    rel_path         TEXT NOT NULL,
    filename         TEXT NOT NULL,
    ext              TEXT NOT NULL,        -- stl, 3mf, lys, lyt, obj, ply, glb, zip, jpg, png, webp...
    storage_state    TEXT NOT NULL CHECK (storage_state IN ('local_resident', 'cloud_placeholder', 'dropbox_remote', 'zip_entry')),
    size_bytes       INTEGER NOT NULL,
    mtime_unix       INTEGER,
    content_hash     TEXT,                 -- BLAKE3 (locale) o content_hash (Dropbox)
    zip_entry_offset INTEGER,              -- Offset per HTTP Range extraction dentro ZIP remoti
    zip_entry_csize  INTEGER,              -- Dimensione compressa dentro ZIP
    zip_entry_method INTEGER,              -- 0=Store, 8=Deflate
    triangle_count   INTEGER,
    bbox_x_mm        REAL,
    bbox_y_mm        REAL,
    bbox_z_mm        REAL,
    thumb_id         INTEGER,
    index_pass       INTEGER NOT NULL DEFAULT 1, -- 1=tree, 2=meta/tax, 3=geometry/render
    CHECK (model_id IS NOT NULL OR (
        ext IN ('jpg', 'jpeg', 'png', 'webp') AND parent_archive_id IS NULL AND storage_state <> 'zip_entry'
    ))
);

CREATE INDEX idx_files_hash ON files(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX idx_files_model ON files(model_id);
CREATE INDEX idx_files_ext ON files(ext);

CREATE TABLE thumbnails (
    id              INTEGER PRIMARY KEY,
    cache_key       TEXT NOT NULL UNIQUE,  -- hash deterministico contenuto o sorgente
    rel_webp_path   TEXT NOT NULL,
    origin          TEXT NOT NULL CHECK (origin IN ('creator_image', '3mf_plate', 'lys_preview', 'wgpu_render', 'dropbox_thumb')),
    frame_count     INTEGER NOT NULL DEFAULT 1, -- 1 = singola, 4 = sprite sheet per hover scrub
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    created_at      INTEGER NOT NULL
);

CREATE TABLE tags (
    id              INTEGER PRIMARY KEY,
    facet           TEXT NOT NULL,         -- print_tech, category, theme, support_state, scale, creator, format
    slug            TEXT NOT NULL,         -- es. 'resin', 'miniature', 'fantasy'
    label_it        TEXT NOT NULL,         -- es. 'Resina', 'Miniatura', 'Fantasy'
    label_en        TEXT NOT NULL,
    UNIQUE(facet, slug)
);

CREATE TABLE model_tags (
    model_id        INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    tag_id          INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    origin          TEXT NOT NULL CHECK (origin IN ('auto', 'user_override')),
    confidence      REAL NOT NULL DEFAULT 1.0,
    is_excluded     INTEGER NOT NULL DEFAULT 0, -- 1 = utente ha rimosso questo auto-tag; non va rigenerato
    evidence_json   TEXT,                       -- es. {"source":"path","token":"Resina","weight":0.95}
    PRIMARY KEY (model_id, tag_id)
);

CREATE INDEX idx_model_tags_tag ON model_tags(tag_id, is_excluded);

-- Tabella di ponte per conservare correzioni utente anche quando una cartella/zip cambia path
CREATE TABLE fingerprint_overrides (
    fingerprint     TEXT NOT NULL,
    tag_id          INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    is_excluded     INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL,
    PRIMARY KEY (fingerprint, tag_id)
);

-- Indice Full-Text bilingue istantaneo
CREATE VIRTUAL TABLE models_fts USING fts5(
    model_id UNINDEXED,
    title,
    path_tokens,
    file_tokens,
    tags_it_en,
    tokenize = 'unicode61 remove_diacritics 2'
);
```
