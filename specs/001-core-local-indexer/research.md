# Research & Technical Decisions: Core Local Indexer (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26  
**Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

Questo documento consolida le decisioni tecniche della Milestone M1, risolvendo ogni scelta implementativa in allineamento con la [Costituzione](../../.specify/memory/constitution.md) e l'architettura master in [docs/02-ARCHITECTURE.md](../../docs/02-ARCHITECTURE.md) e [docs/04-UI-UX-AND-IPC.md](../../docs/04-UI-UX-AND-IPC.md).

---

## 1. Struttura Repository Cross-OS (`WSL2 <-> Windows`)

- **Decisione:** Strutturare il backend Rust come **Cargo Workspace** dentro `src-tauri/` con due crate:
  1. `meshcatalog-core` (`src-tauri/crates/meshcatalog-core`): libreria pure-Rust indipendente da Tauri/WebView2 contenente schema & migrazioni SQLite, scanner filesystem con trait `FileAttrInspector`, reader Central Directory `.zip`, hasher `blake3`, logica di raggruppamento `folder_pack`/`zip_archive`/`single_file`, pulizia `display_title`, query `FTS5`, e orchestratore di pipeline Pass 1 / Pass 2.
  2. `meshcatalog-app` (`src-tauri/`): binario desktop **Tauri v2** che integra `meshcatalog-core`, gestisce lo stato asincrono (`tokio` + `tokio-util`), i comandi IPC, il dialog nativo di selezione cartella e l'emissione di eventi verso il frontend React.
- **Motivazione:** Su WSL2 Linux (ambiente di authoring senza dipendenze GUI Windows/WebView2 né `webkit2gtk` installati), `cargo test -p meshcatalog-core` esegue l'intera suite unitaria e di integrazione locale in pochi secondi, mentre su Windows (`x86_64-pc-windows-msvc`) `cargo tauri dev` compila l'app completa con supporto Win32 nativo (Principio V).
- **Alternative considerate:**
  - *Crate unico `src-tauri` con moduli interni:* Rifiutato perché `tauri` nel `Cargo.toml` richiede librerie di sistema GUI Linux (`webkit2gtk-4.1`, `libayatana-appindicator`) per compilare o eseguire `cargo test` in WSL2.

---

## 2. Scudo Anti-Idratazione Win32 & Traversal Sicuro NTFS

- **Decisione:**
  - `EntryMetadataInspector` implementa il [guard Win32](../../docs/02-ARCHITECTURE.md#21-scudo-anti-idratazione-su-disco-locale-win32-guard). Fuori Windows usa `std::fs::symlink_metadata`.
  - Il walker e il guard di apertura verificano radice e antenati. Le directory con bit di richiamo/offline rimangono opache: non vengono enumerate e il contenuto storico viene conservato.
  - `MockMetadataInspector` simula attributi e tag per testare file e directory residenti, segnaposto e reparse point senza accedere a Dropbox.
- **Motivazione:** Garantisce **0 byte** scaricati da Dropbox Smart Sync (Principio II / SC-001) e impedisce loop infiniti o uscite dalla radice autorizzata su junction NTFS (FR-005).
- **Alternative considerate:**
  - *Soli attributi via `MetadataExt` o `GetFileAttributesW`:* Non bastano a distinguere un reparse point cloud residente da un reparse point sconosciuto o da una junction. `FindFirstFileW` espone il tag in `WIN32_FIND_DATAW.dwReserved0`.

---

## 3. Regole di Esclusione, Raggruppamento Pack e Pulizia Titoli

- **Decisione:**
  - **Esclusioni su disco** (confronto case-insensitive sul nome componente):
    - Directory/file di sistema e sync: `$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`.
    - Metadati macOS: `__MACOSX` e qualsiasi file con prefisso `._`.
  - **Esclusioni dentro `.zip`:** qualsiasi entry con segmento di path `__MACOSX` o filename che inizia con `._`.
  - **Raggruppamento deterministico (FR-007):**
    1. Ogni `.zip` (residente o placeholder) produce esattamente un `Model` con `container_kind = 'zip_archive'`. Se residente, le sue entry supportate diventano `CatalogFile` con `storage_state = 'zip_entry'` e `parent_archive_id` valorizzato.
    2. Le sottocartelle dirette di una cartella `P` (purché `P` non sia la radice sorgente) e i relativi discendenti formano un **sottoalbero assorbibile** se soddisfano le condizioni di FR-007: (a) nessuna cartella nel sottoalbero è opaca, inosservata o contiene archivi `.zip`, e (b) ogni nodo è una cartella tecnica nota (`stl`, `stls`, `lys`, `3mf`, `presupported`, `pre-supported`, `supported`, `unsupported`, `images`, `renders`, `render`, `lychee`, `files`, scale `^\d+(?:[.,]\d+)?\s*mm$` / `^1[_-]\d+$`), una variante/posa esplicita (`Pose 1`, `Var A`, `v2`), oppure una sigla posa di 1–2 caratteri alfanumerici già contestualizzata (sotto un nodo tecnico/posa, in una cartella `P` con file diretti, o con figli tecnici propri). I file contenuti nell'intero sottoalbero vengono aggregati al `folder_pack` di `P` e i nodi assorbiti non vengono pubblicati nell'albero. I batch mantengono ricercabili i file dei nodi candidati tramite `folders.is_visible` (migrazione v3, rivelazione ricorsiva degli antenati) e la CTE ricorsiva.
    3. Sui file non-`.zip` raccolti in `P` (diretti + assorbiti):
       - `count(3d_slicer) >= 2` oppure (`count(3d_slicer) == 1 && count(images) >= 1`) $\rightarrow$ `folder_pack`.
       - `count(3d_slicer) == 1 && count(images) == 0` $\rightarrow$ `single_file`, salvo foglia non tecnica con unico membro in una sottocartella assorbita (`folder_pack` intitolato e collocato come da FR-007).
       - `count(3d_slicer) == 0` $\rightarrow$ nessun `Model` 3D per le sole immagini non associate. Tali immagini rimangono in `files` con `model_id = NULL` e in `files_fts`, nello scope di cartella e sorgente.
  - **Pulizia `display_title`:** Pipeline deterministica in Rust (`title.rs`) che normalizza separatori (`_`, `.`), rimuove prefissi data/release (`2025_03_`, `[Creator]`), rimuove token tecnici (`presupported`, `unsupported`, `32mm`, `v2`, `stl`, `3mf`), prefissa eventuali titoli bare di posa (`A`, `Pose 1`) col genitore non tecnico più vicino (`Squad - A`), preserva una leggibilità naturale e ricade sempre su `raw_name` se la stringa ripulita risultasse vuota.
- **Motivazione:** Rispecchia esattamente la Chiarificazione 1 & 5 di `spec.md`, evita di esplodere un singolo modello in decine di card o cartelle tecniche `STL/` / `Presupported/` nell'albero navigabile, e garantisce compatibilità immediata con il futuro Segnale 3 del Motore Tassonomico (M3).

---

## 4. Storage SQLite (`WAL` + `FTS5`), Concorrenza e Aggregati Ricorsivi Cartelle

- **Decisione:**
  - Utilizzare `rusqlite` con feature `bundled` (SQLite >= 3.45 con `FTS5` compilato staticamente), `PRAGMA journal_mode = WAL`, `PRAGMA foreign_keys = ON`, `PRAGMA synchronous = NORMAL`, `PRAGMA busy_timeout = 5000`.
  - Adottare uno schema compatibile fin da M1 con il DDL master di `docs/02-ARCHITECTURE.md` (`sources`, `folders`, `models`, `files`, `thumbnails`, `tags`, `model_tags`, `fingerprint_overrides`, `models_fts`), con migrazione v2 per le immagini non associate ed estensioni additive a `folders`, `sources` e `models` per i requisiti M1 (`recursive_model_count`, `recursive_file_count`, `nested_folder_count`, `sample_titles_json`, `has_cloud_placeholder`, `primary_ext`) più `files_fts` per rendere istantanea (`< 100 ms`) anche la ricerca in vista **Singoli File**.
  - **Separazione Writer / Readers:** Un singolo task/thread writer dedicato esegue le scritture batchate in transazioni SQLite (`500–1000` record per commit) durante la scansione; un pool leggero di connessioni `SQLITE_OPEN_READ_ONLY` serve le query IPC della UI senza mai bloccare lo scorrimento o la ricerca.
  - **Ricerca ricorsiva per sotto-albero:** Dato un `folder_id` selezionato, una CTE ricorsiva `WITH RECURSIVE subtree(id) AS (...)` su `folders(parent_id)` (indicizzata) combinata con `models_fts MATCH ?` / `files_fts MATCH ?` isola il sotto-albero in `< 1 ms`, mantenendo latenza totale `< 30–50 ms` su 50k+ voci.
- **Motivazione:** Garantisce il rispetto del budget `< 100 ms` (SC-006) anche mentre Pass 1/Pass 2 stanno scrivendo in background (FR-010).
- **Alternative considerate:**
  - *Singola `Mutex<Connection>` condivisa tra scanner e UI:* Rifiutata perché transazioni di bulk insert potrebbero introdurre micro-blocchi sui comandi `search_catalog` della UI.

---

## 5. Ispezione `.zip` senza Estrazione, Hashing `BLAKE3` e Resilienza a Spostamenti

- **Decisione:**
  - **ZIP Central Directory (`zip` crate):** Aprire i soli `.zip` con `storage_state == 'local_resident'` tramite `zip::ZipArchive::new(BufReader::new(file))`, leggere solo metadati di indice (`by_index_raw` / `name()`, `size()`, `compressed_size()`, `compression()`, `data_start()` se disponibile o offset metadata), **senza chiamare `read()` sul contenuto decompresso** e senza scrivere su disco. Archivi protetti da password, troncati o corrotti vengono intercettati con `Result::Err`, registrano un evento recuperabile `indexer://error` e mantengono il record `zip_archive` come archivio singolo non espanso. Eventuali `.zip` annidati restano catalogati come `zip_entry` con `ext = "zip"` senza decompressione ricorsiva.
  - **Hashing incrementale & Move Matching:**
    - In Pass 2, ogni file `local_resident` con `(size_bytes, mtime_unix)` identici al DB e `content_hash IS NOT NULL` salta il calcolo `BLAKE3` (SC-004: `< 5s` su 40 GB invariati).
    - I nuovi file o file modificati vengono hashati in streaming (`blake3::Hasher::update_reader`, buffer 64 KB) usando un thread pool `rayon` dimensionato (`num_cpus.min(8)`).
    - Ogni `Model` calcola un `fingerprint` stabile (hash `BLAKE3` del singolo file, dello `.zip`, o della lista ordinata dei `(content_hash, size_bytes)` dei file membri di un `folder_pack`).
    - La finalizzazione separa i risultati osservati dal pruning generale. Se la radice è accessibile, può riconciliare una rinomina certa 1:1 anche con un altro sottoalbero opaco o inaccessibile. Le righe sotto percorsi non osservati non sono candidate, e un pack incompleto conserva l'impronta precedente. Vedere [riconciliazione e `FTS5`](./data-model.md#26-riconciliazione-spostamenti-e-sincronizzazione-fts5).
- **Motivazione:** Soddisfa SC-003 (`< 60s` prima scansione 40 GB), SC-004 (`< 5s` ri-scansione invariata) e SC-005 (`100%` preservazione identità su spostamenti/rinomine).

---

## 6. Frontend Virtualizzato (`React 19` + `@tanstack/react-virtual`) & Inspector Testuale M1

- **Decisione:**
  - Tema scuro neutro (`#121417` background, `#1B1F24` card/pannelli, `#272C33` bordi, badge distintivi per `Cloud / Online-Only`, `ZIP`, estensioni 3D).
  - Virtualizzazione griglia/lista con `@tanstack/react-virtual` (calcolo dinamico colonne su resize del container, riga virtualizzata per griglia a card o tabella per `Singoli File`) garantendo frame time `< 16 ms`.
  - Stato navigazione centralizzato: `(activeSourceFilter, activeFolderId, viewMode: 'models' | 'files', searchText, selectedModelId)`. Lo switch `Modelli / Pack` $\leftrightarrow$ `Singoli File` cambia solo `viewMode` preservando `activeFolderId` e `searchText` e ri-interrogando `search_catalog`.
  - Zero canvas WebGL/Three.js e zero decodifica immagini in M1; l'Inspector laterale carica esclusivamente `get_model_detail(modelId)` (testo + lista completa `CatalogFile`).
- **Motivazione:** Rispetta il Principio III (Zero-Latency 2D), FR-011, FR-013, FR-014 e i confini di milestone M1 (M2 introdurrà thumbnail `.webp` e viewer 3D on-demand).
