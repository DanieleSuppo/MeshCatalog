---

description: "Actionable, dependency-ordered task list for Milestone M1: Core Local Indexer & Baseline Browser (001-core-local-indexer)"
---

# Tasks: Core Local Indexer & Baseline Browser (`001-core-local-indexer`)

**Input**: Design documents from `/specs/001-core-local-indexer/`
**Prerequisites**: `plan.md` (required), `spec.md` (required), `data-model.md`, `contracts/tauri-ipc.md`, `research.md`, `quickstart.md`, `.specify/memory/constitution.md`

**Tests**: Included explicitly for `meshcatalog-core` (pure-Rust unit & integration tests verifiable on WSL2 Linux via `cargo test`) and frontend type-check/build verification (`tsc --noEmit` + Vite) per `plan.md` and `quickstart.md`.

**Organization**: Tasks are grouped by Setup, Foundational prerequisites, User Stories in priority order (`US1` [P1], `US2` [P1], `US4` [P1], `US3` [P2]), and final Polish & Cross-OS Verification.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story mapping (`[US1]`, `[US2]`, `[US4]`, `[US3]`; omitted in Setup, Foundational, and Polish phases)
- Exact file paths are specified in every task description.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Initialize the Tauri v2 + Cargo Workspace (`meshcatalog-core` + `meshcatalog-app`) and the React 19 + TypeScript + Vite frontend scaffold.

- [X] T001 Create Cargo workspace manifest (`members = [".", "crates/meshcatalog-core"]`) and Tauri v2 dependencies (`tauri`, `tauri-plugin-dialog`, `tokio`, `tokio-util`, `serde`, `serde_json`, `thiserror`) in `src-tauri/Cargo.toml`
- [X] T002 [P] Create pure-Rust core crate manifest (`rusqlite` with `bundled`, `blake3`, `zip`, `walkdir`, `rayon`, `serde`, `serde_json`, `thiserror`, `regex`) in `src-tauri/crates/meshcatalog-core/Cargo.toml`
- [X] T003 [P] Initialize frontend package manifest (`react` 19, `react-dom` 19, `@tanstack/react-virtual`, `@tauri-apps/api` v2, `typescript`, `vite`, `@vitejs/plugin-react`, `@tauri-apps/cli`) in `package.json`
- [X] T004 [P] Configure TypeScript compiler options (`strict: true`, JSX `react-jsx`) in `tsconfig.json`, Vite dev/build config in `vite.config.ts`, and HTML entry in `index.html`
- [X] T005 [P] Configure Tauri v2 application metadata, window defaults, and `app_local_data_dir` identifier in `src-tauri/tauri.conf.json` and read-only dialog/core ACL permissions in `src-tauri/capabilities/default.json`
- [X] T006 [P] Create dark studio theme CSS variables and base layout utility styles (`#121417` background, `#1B1F24` cards/panels, `#272C33` borders, format/`Cloud / Online-Only` badge tokens) in `src/styles/theme.css`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core database schema, error types, shared IPC DTO contracts, Tauri app bootstrap (`%LOCALAPPDATA%\MeshCatalog`), and TypeScript IPC client that MUST be complete before any User Story implementation begins.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T007 Author SQLite initial DDL migration (`sources`, `folders`, `models`, `files`, M2/M3 forward tables `thumbnails`, `tags`, `model_tags`, `fingerprint_overrides` per `docs/02-ARCHITECTURE.md` §5, `models_fts`, `files_fts`, and B-Tree indexes) in `src-tauri/crates/meshcatalog-core/migrations/001_initial_schema.sql` preserving exact constraints (`kind IN ('local_dir', 'dropbox')`, `status IN ('idle', 'scanning', 'unavailable', 'error')`, `container_kind IN ('folder_pack', 'zip_archive', 'single_file')`, `storage_state IN ('local_resident', 'cloud_placeholder', 'dropbox_remote', 'zip_entry')`, `UNIQUE(source_id, rel_path)`, `UNIQUE(root_uri)`, `UNIQUE(canonical_uri)`, `UNIQUE(uri)`, `tokenize = 'unicode61 remove_diacritics 2'`)
- [X] T008 [P] Implement domain & IPC error enum (`IpcError` with `IpcErrorCode` variants `'SOURCE_NOT_FOUND' | 'SOURCE_UNAVAILABLE' | 'SOURCE_OVERLAP' | 'INVALID_PATH' | 'SCAN_ALREADY_RUNNING' | 'MODEL_NOT_FOUND' | 'DATABASE_ERROR' | 'INTERNAL_ERROR'`) in `src-tauri/crates/meshcatalog-core/src/error.rs`
- [X] T009 [P] Define shared `serde(rename_all = "camelCase")` DTOs (`ViewMode`, `SourceFilter`, `SourceKind`, `SourceStatus`, `ContainerKind`, `StorageState`, `SourceDto`, `FolderTreeNode`, `FolderCardDto`, `ModelCardDto` using `relativePath`, `FileCardDto` using `relPath`, `#[serde(untagged)] enum SearchItemDto`, `SearchQuery`, `SearchResultPage`, `ModelDetailDto`, `ScanStatusDto`, and `indexer://*` event payloads) in `src-tauri/crates/meshcatalog-core/src/contracts.rs`
- [X] T010 Implement SQLite connection manager (`PRAGMA journal_mode = WAL`, `PRAGMA foreign_keys = ON`, `PRAGMA synchronous = NORMAL`, `PRAGMA busy_timeout = 5000`, dedicated writer connection + `SQLITE_OPEN_READ_ONLY` reader pool) in `src-tauri/crates/meshcatalog-core/src/db/mod.rs` and versioned migration runner in `src-tauri/crates/meshcatalog-core/src/db/migrations.rs`
- [X] T011 Expose core module structure (`db`, `scanner`, `archive`, `hasher`, `pipeline`, `contracts`, `error`) in `src-tauri/crates/meshcatalog-core/src/lib.rs`
- [X] T012 [P] Create TypeScript DTO definitions matching `contracts/tauri-ipc.md` in `src/types/catalog.ts` and typed Tauri `invoke`/`listen` client wrapper (`list_sources`, `add_local_source`, `remove_source`, `pick_local_folder`, `start_index_scan`, `cancel_index_scan`, `get_scan_status`, `get_folder_tree`, `search_catalog`, `get_model_detail`) in `src/api/tauriClient.ts`
- [ ] T013 Bootstrap Tauri v2 desktop binary (`app_local_data_dir` initialization under `%LOCALAPPDATA%\MeshCatalog` for `catalog.db` and `tauri::State` app skeleton; IPC handlers wired in `T021`/`T032`) in `src-tauri/src/main.rs` and `src-tauri/src/lib.rs`

**Checkpoint**: Foundation ready — `catalog.db` migrates cleanly with `WAL` + `FTS5`, shared Rust/TS contracts align with `contracts/tauri-ipc.md`, and Tauri app boots using `app_local_data_dir` only.

---

## Phase 3: User Story 1 - Gestione sorgenti locali in sola lettura e scansione sicura anti-idratazione (Priority: P1) 🎯 MVP

**Goal**: Allow users to register/list/remove local root folders (preventing duplicates/overlaps without ever writing inside user folders) and run a non-blocking background Pass 1 scan equipped with the Win32 Anti-Hydration Guard so `Online-Only` cloud placeholders are cataloged from metadata alone with **0 bytes** downloaded.

**Independent Test**: Run `cargo test -p meshcatalog-core --test anti_hydration_tests` to verify mocked Win32 bitmasks (`0x0040_0000`, `0x0000_1000`, `0x0004_0000`, `0x0400` reparse point), zero `File::open` calls on placeholders, symlink/junction skipping, system junk exclusion, source overlap rejection, and `remove_source` cascade preserving disk files.

### Tests for User Story 1 (WSL2 Pure-Rust Verification)

- [X] T014 [P] [US1] Write unit & integration tests for Win32 Anti-Hydration Guard bitmasks (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS = 0x0040_0000`, `FILE_ATTRIBUTE_OFFLINE = 0x0000_1000`, `FILE_ATTRIBUTE_RECALL_ON_OPEN = 0x0004_0000`, `FILE_ATTRIBUTE_REPARSE_POINT = 0x0400` with vs without recall bits), mock `EntryMetadataInspector`, zero read-handle opening on `cloud_placeholder`, symlink/junction directory skip, and source overlap validation in `src-tauri/crates/meshcatalog-core/tests/anti_hydration_tests.rs`

### Implementation for User Story 1

- [X] T015 [P] [US1] Implement `EntryMetadataInspector` trait, `RawWin32AttrFlags`, `OsMetadataInspector` (using Win32 metadata lookup for attributes and reparse tags; `std::fs::symlink_metadata` off Windows), and `MockMetadataInspector` test double in `src-tauri/crates/meshcatalog-core/src/scanner/attr_guard.rs`
- [X] T016 [P] [US1] Implement case-insensitive system/cache/macOS exclusion rules (`$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`, `__MACOSX`, prefix `._*`) and supported extension registry (`stl`, `3mf`, `lys`, `lyt`, `obj`, `ply`, `glb`, `step`, `stp`, `gcode`, `zip`, `jpg`, `jpeg`, `png`, `webp`) in `src-tauri/crates/meshcatalog-core/src/scanner/exclusions.rs`
- [X] T017 [US1] Implement safe directory walker in `src-tauri/crates/meshcatalog-core/src/scanner/walker.rs` and export scanner module in `src-tauri/crates/meshcatalog-core/src/scanner/mod.rs`. Use `std::fs::read_dir` only after `EntryMetadataInspector` and exclusion checks; `WalkDir` opens directories before invoking its filter, so it cannot enforce that order.
- [X] T018 [US1] Implement source CRUD (`list_sources`, `add_local_source` with canonical path normalization + ancestor/descendant/exact overlap check returning `IpcError::SourceOverlap`, `remove_source` with `ON DELETE CASCADE`, `mark_source_status`) in `src-tauri/crates/meshcatalog-core/src/db/sources.rs`
- [X] T019 [US1] Implement Pass 1 inventory writer (transactional batches of 500–1000 records inserting root/directory `folders` with root `name = sources.name`, provisional `models` & `files` later refined by `grouping.rs` in `T027`, setting `storage_state = 'cloud_placeholder'` and `index_pass = 2` for placeholders vs `local_resident` and `index_pass = 1` for resident files, plus explicit `models_fts` / `files_fts` population) in `src-tauri/crates/meshcatalog-core/src/db/writer.rs`
- [X] T020 [US1] Implement background pipeline Pass 1 runner (`PipelineSink` callbacks for progress/batch/error, inaccessible-source protection setting `status = 'unavailable'` without deleting historical rows, `CancellationToken` checks) in `src-tauri/crates/meshcatalog-core/src/pipeline/mod.rs`
- [ ] T021 [US1] Implement async `ScannerService` (`tokio` task orchestration, per-source scan lock returning `SCAN_ALREADY_RUNNING`, `indexer://progress`, `indexer://batch_updated`, `indexer://error`, `indexer://completed` event emission) in `src-tauri/src/scanner_service.rs` and wire `list_sources`, `add_local_source`, `remove_source`, `pick_local_folder`, `start_index_scan`, `cancel_index_scan`, `get_scan_status` in `src-tauri/src/commands.rs`
- [X] T022 [US1] Implement initial source management sidebar UI (`+ Aggiungi cartella` via `pick_local_folder` with user-facing `SOURCE_OVERLAP`/`INVALID_PATH` error feedback, source list with `status`/`lastError`/`modelCount`/`fileCount`, manual `Scansiona` and `Rimuovi` actions) in `src/components/SourceSidebar.tsx` and basic scan progress pill + `Scansiona tutto` action (`start_index_scan(undefined)`) in `src/components/TopCommandBar.tsx`

**Checkpoint**: User Story 1 (MVP) is complete and testable independently — local sources can be registered/removed without modifying user disks, and Pass 1 catalogs resident and `Online-Only` placeholders safely with 0 bytes hydrated.

---

## Phase 4: User Story 2 - Indicizzazione trasparente di cartelle, archivi `.zip` e formati 3D (Priority: P1)

**Goal**: Discover and group all supported 3D, slicer, image, and `.zip` assets deterministically into `folder_pack` (absorbing terminal technical subfolders), `zip_archive` (reading `.zip` Central Directory without extracting to disk), and `single_file`, while cleaning noisy names into `display_title` and computing recursive folder subtree aggregates.

**Independent Test**: Run `cargo test -p meshcatalog-core --test grouping_and_exclusions_tests` and `cargo test -p meshcatalog-core --test zip_and_incremental_tests` (grouping & `.zip` cases) to verify technical subfolder absorption, `display_title` cleanup with fallback to `raw_name`, non-extracting `.zip` Central Directory parsing, `__MACOSX/`/`._*` filtering inside `.zip`, nested `.zip` handling, and corrupt/encrypted `.zip` recovery.

### Tests for User Story 2 (WSL2 Pure-Rust Verification)

- [X] T023 [P] [US2] Write unit & integration tests for deterministic grouping (`zip_archive`, `folder_pack` via `>= 2` 3D/slicer or `1` 3D/slicer + `>= 1` image, `single_file`, zero spurious image-only models), terminal technical subfolder absorption (`stl`, `stls`, `presupported`, `pre-supported`, `supported`, `unsupported`, `images`, `renders`, `render`, `lychee`, `files`, scale regex `^\d+(?:[.,]\d+)?\s*mm$` and `^1[_-]\d+$` like `32mm`/`75mm`/`1_10`), source-root non-absorption guard when root has no direct files, non-absorption when subfolders contain sub-dirs or `.zip`, and `display_title` cleanup (preserving non-empty fallback to `raw_name`) in `src-tauri/crates/meshcatalog-core/tests/grouping_and_exclusions_tests.rs`
- [X] T024 [P] [US2] Write unit & integration tests for local `.zip` Central Directory reading without disk extraction (`zip_entry` rows with `parent_archive_id`, `uri = zip://<parent_uri>#<internal_rel_path>`, uncompressed `size_bytes`, `zip_entry_csize`, `zip_entry_method`), exclusion of `__MACOSX/` and `._*` inside `.zip`, nested `.zip` recorded as `ext = 'zip'` without recursion, and corrupt/password-protected `.zip` emitting recoverable error while keeping `zip_archive` model in `src-tauri/crates/meshcatalog-core/tests/zip_and_incremental_tests.rs`

### Implementation for User Story 2

- [X] T025 [P] [US2] Implement deterministic `display_title` cleaner (`_`/`.` normalization, date/release prefix stripping like `2025_03_`, `[Creator]` brackets, technical tokens `presupported`/`unsupported`/`32mm`/`v2`/`stl`/`3mf` stripping, non-empty fallback to `raw_name`) in `src-tauri/crates/meshcatalog-core/src/scanner/title.rs`
- [X] T026 [P] [US2] Implement local `.zip` Central Directory reader (`zip::ZipArchive::new(BufReader::new(file))` reading index metadata via `by_index_raw` without decompressing entry bytes, filtering `__MACOSX/` and `._*`, handling corrupt/encrypted archives as recoverable warnings) in `src-tauri/crates/meshcatalog-core/src/archive/zip_reader.rs` and `src-tauri/crates/meshcatalog-core/src/archive/mod.rs`
- [X] T027 [US2] Implement deterministic folder/model grouping engine (source-root non-absorption guard + terminal technical subfolder absorption for `stl`/`stls`/`presupported`/`pre-supported`/`supported`/`unsupported`/`images`/`renders`/`render`/`lychee`/`files` and `^\d+(?:[.,]\d+)?\s*mm$`/`^1[_-]\d+$`, `folder_pack` / `zip_archive` / `single_file` classification, leaf-pack attachment rule `models.folder_id = id(parent(P))` without extra `folders` row vs mixed container `models.folder_id = id(P)`, `primary_ext` & `has_cloud_placeholder` derivation, non-technical `folders` tree construction) in `src-tauri/crates/meshcatalog-core/src/scanner/grouping.rs`
- [X] T028 [US2] Add migration `002_allow_unassociated_images.sql` for nullable `files.model_id` only on unassociated images, preserving catalog IDs, ZIP relationships, FTS and overrides; align IPC `FileCardDto.modelId` and verify migration in `tests/foundation_db.rs`. Extend `writer.rs` to persist `.zip` `zip_entry` rows (`storage_state = 'zip_entry'`, `parent_archive_id`, `index_pass = 2`), update parent `zip_archive` `file_count`/`total_bytes`, sync `models_fts`/`files_fts` with internal `.zip` filenames, and recompute bottom-up `folders` aggregates (`model_count`, `file_count`, `recursive_model_count`, `recursive_file_count`, `nested_folder_count`, `sample_titles_json` of 3–4 titles, `summary_json` including `"cloud"` count) in `src-tauri/crates/meshcatalog-core/src/db/writer.rs`
- [X] T029 [US2] Integrate grouping, title cleanup, Pass 2 `.zip` Central Directory inspection (strictly gated to `storage_state == 'local_resident'`), and folder aggregate recalculation into `src-tauri/crates/meshcatalog-core/src/pipeline/mod.rs`

**Checkpoint**: User Story 2 is complete — multi-file packs, technical subfolders, and resident `.zip` archives are transparently cataloged with clean titles and recursive folder stats without extracting a single byte to user directories.

---

## Phase 5: User Story 4 - Navigazione gerarchica, switch Modelli/File e ricerca istantanea (Priority: P1)

**Goal**: Provide a responsive 4-area desktop UI (`#121417` / `#1B1F24` / `#272C33`) where users browse the non-technical folder tree, explore mixed subfolders (with recursive counts & 3–4 sample titles) + direct items, switch instantaneously between `Modelli / Pack` (default) and `Singoli File`, search 50,000+ entries via `FTS5` in `< 100 ms` (recursively within the selected folder subtree), and inspect model metadata + full file list in a lightweight read-only side Inspector without blocking on active background scans.

**Independent Test**: Run `cargo test -p meshcatalog-core --test fts5_perf_50k_tests` to verify 50,000+ record insertion, root-folder & mixed folder/model browsing queries, recursive subtree FTS5 search (`< 100 ms`, including punctuation-escaped queries) across both `models` and `files` view modes, and `get_model_detail` ordering; run `npm run build` and verify virtualized UI rendering with zero WebGL/Three.js dependencies.

### Tests for User Story 4 (WSL2 Pure-Rust Verification)

- [X] T030 [P] [US4] Verify standalone images in File view, scoped FTS, pagination, rescan and source removal. Write integration & performance benchmark tests populating 50,000+ `models`/`files` records and asserting `< 100 ms` latency for `search_catalog` in both `viewMode = 'models'` and `viewMode = 'files'` (global and scoped to `folder_id` via `WITH RECURSIVE subtree`, plus FTS5 punctuation queries like `King-Thror` and `.stl`), root browsing when `folderId === null && text.trim() === ""` returning `parent_id IS NULL` source root `subfolders` + `items = []`, mixed subfolders + direct items when `folderId != null && text.trim() === ""` (`recursive_model_count`, `recursive_file_count`, `nested_folder_count`, `sample_titles_json`), `get_folder_tree` exclusion of absorbed technical folders, and `get_model_detail` complete file list in `src-tauri/crates/meshcatalog-core/tests/fts5_perf_50k_tests.rs`

### Implementation for User Story 4

- [X] T031 [US4] Implement read-only SQLite query layer (`get_folder_tree`, `search_catalog` handling `folderId === null && text.trim() === ""` root folders, `folderId != null && text.trim() === ""` direct `subfolders` + `items`, and recursive `WITH RECURSIVE subtree` `models_fts`/`files_fts` search with FTS5 token sanitization/quoting for `-`, `.`, `"`, `*` and contextual `relativePath`/`relPath`, plus `get_model_detail`) in `src-tauri/crates/meshcatalog-core/src/db/queries.rs`
- [ ] T032 [US4] Expose `get_folder_tree`, `search_catalog`, and `get_model_detail` Tauri IPC commands using the read-only SQLite pool in `src-tauri/src/commands.rs`
- [X] T033 [P] [US4] Extend `src/components/SourceSidebar.tsx` with collapsible non-technical folder tree (`FolderTreeNode` hierarchy, recursive model counts, active selection highlighting mapping source/folder selection to `(activeSourceId, activeFolderId)` or `null` for `All Sources`, and `All Sources` / `Local` filter)
- [X] T034 [P] [US4] Extend `src/components/TopCommandBar.tsx` with instant search input, `Modelli / Pack` (default) vs `Singoli File` segmented toggle preserving `activeFolderId` and `searchText`, `Scansiona tutto` button (`start_index_scan(undefined)`), query timing display (`queryTimeMs`), and non-blocking `indexer://progress` + cancel + recoverable error indicator
- [X] T035 [P] [US4] Create `src/components/FolderCard.tsx` displaying non-technical subfolder name, `recursiveModelCount`, `recursiveFileCount`, `nestedFolderCount`, 3–4 `sampleTitles`, and `hasCloudPlaceholder` badge
- [X] T036 [P] [US4] Create `src/components/ModelOrFileCard.tsx` rendering format/container badges (`ZIP`, `3MF`, `STL`, `LYS`, etc.), `Cloud / Online-Only` badge, `displayTitle`/`filename`, contextual path (`ModelCardDto.relativePath` or `FileCardDto.relPath`), `fileCount`, and formatted `totalBytes`/`sizeBytes` (zero WebGL/image decoding)
- [X] T037 [US4] Implement virtualized `src/components/CatalogGrid.tsx` using `@tanstack/react-virtual` (responsive column calculation via `ResizeObserver`, distinct Subfolders section and Models/Files section when `text` is empty, recursive search results list/grid when `text` is active, paged `limit`/`offset` windowing via `search_catalog`, `< 16 ms` scroll budget)
- [X] T038 [P] [US4] Implement lightweight read-only side Inspector `src/components/TextInspectorPanel.tsx` calling `get_model_detail(modelId)` to show `displayTitle`, `rawName`, `canonicalUri`, `containerKind`, `primaryExt`, `totalBytes`, `fileCount`, and scrollable ordered `files` list (`relPath`, `ext`, `sizeBytes`, `storageState`); do not invoke `get_model_detail` for cards with `modelId === null`
- [X] T039 [US4] Wire 4-area desktop layout, centralized navigation state `(activeSourceFilter, activeSourceId, activeFolderId, viewMode, searchText, selectedModelId)`, debounced/instant `search_catalog` invocation, and `indexer://progress` / `indexer://batch_updated` / `indexer://error` / `indexer://completed` listeners in `src/App.tsx` and entry point in `src/main.tsx`

**Frontend verification (T033–T039):** `npm test` exercises the mounted app against synthetic Tauri IPC, including scoped navigation, stale responses, model-less files, page retry/reuse, scan events and Inspector dismissal. `npm run build` checks TypeScript and produces the Vite bundle. For manual layout checks, run `npm run dev` and open `/tests/fixtures/catalog.html` (50,010 synthetic models; no filesystem access). Browser geometry checked at 1280×800, 1024×650 and 390×720. At 1280×800, 80 synthetic scroll updates measured 5.3 ms p95 synchronous dispatch/layout work, with 24 cards mounted at offset 19,750 px. This does not measure native WebView2 frame presentation; that gate remains T047.

**Checkpoint**: User Story 4 is complete — users can browse mixed folders/models, switch `Modelli / Pack` ↔ `Singoli File`, run `< 100 ms` recursive FTS5 searches over 50k+ items, and inspect pack/ZIP contents in the text Inspector even while scans run in the background.

---

## Phase 6: User Story 3 - Ri-scansione incrementale rapida e resilienza a spostamenti e rinomine (Priority: P2)

**Goal**: Make subsequent re-scans skip 100% of unchanged resident files by `(size_bytes, mtime_unix)`, compute streaming `BLAKE3` hashes + stable pack `fingerprint`s for new/changed files on a `rayon` pool, preserve `models.id` across local moves/renames, and cleanly remove deleted records only when the root source is accessible.

**Independent Test**: Run `cargo test -p meshcatalog-core --test zip_and_incremental_tests` (incremental & move cases) to verify 0 re-hashes when `(size_bytes, mtime_unix)` match, `BLAKE3` recalculation on size/mtime change, `models.id` preservation when moving/renaming a `folder_pack` or `.zip`, orphan cleanup on deletion, and historical retention when a source root is unavailable.

### Tests for User Story 3 (WSL2 Pure-Rust Verification)

- [X] T040 [P] [US3] Expand `src-tauri/crates/meshcatalog-core/tests/zip_and_incremental_tests.rs` with incremental skip verification (mock/instrumented hasher asserting 0 hash reads on unchanged `(size_bytes, mtime_unix)`), modified-file re-hash verification, `folder_pack` & `.zip` move/rename `models.id` preservation via `fingerprint` (including `NULL` fingerprint for placeholder-only models and ambiguity skip on duplicate fingerprints), deleted file/model removal, and unreachable-source historical preservation

### Implementation for User Story 3

- [X] T041 [P] [US3] Implement streaming `BLAKE3` file hasher (`blake3::Hasher::update_reader` with 64 KB buffer, `(size_bytes, mtime_unix)` skip predicate, and deterministic `Model::fingerprint` returning `None` when no resident member has `content_hash` or hashing sorted member `(content_hash, size_bytes)` otherwise) in `src-tauri/crates/meshcatalog-core/src/hasher/blake3_hasher.rs` and `src-tauri/crates/meshcatalog-core/src/hasher/mod.rs`
- [X] T042 [US3] Extend `src-tauri/crates/meshcatalog-core/src/db/writer.rs` with incremental file lookup by `uri`/`(size_bytes, mtime_unix)`, atomic 1:1 `fingerprint` move/rename reconciliation (only when `missing.len() == 1 && new.len() == 1` for a non-null `fingerprint`; deleting any provisional Pass 1 destination row first inside the same transaction before updating `canonical_uri`, `folder_id`, `raw_name`, `display_title` on the preserved `models.id`, with explicit `models_fts`/`files_fts` sync, replacing only changed projections), and orphan deletion restricted strictly to successfully scanned accessible sources
- [X] T043 [US3] Integrate Pass 2 parallel `rayon` (`num_cpus.min(8)`) `BLAKE3` hashing for uncached/changed `local_resident` files, complete-model `fingerprint` computation, verified move reconciliation on partial walks, and orphan pruning gated on traversal completeness into `src-tauri/crates/meshcatalog-core/src/pipeline/mod.rs`

**Checkpoint**: User Story 3 is complete — unchanged re-scans skip 100% of file hashing, moved/renamed packs and `.zip` archives keep their catalog `id`, and deleted items are pruned safely.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final cross-OS verification (`WSL2` pure-Rust & TypeScript build + native `Windows 10/11` `x86_64-pc-windows-msvc` validation per `quickstart.md`), linting, and compliance audit against Constitution Principles I–V.

- [X] T044 [P] Run full WSL2 Rust verification (`cargo fmt --check --manifest-path src-tauri/Cargo.toml`, `cargo clippy --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --all-targets -- -D warnings`, `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core`) and fix any warnings or test failures across `src-tauri/crates/meshcatalog-core/`
- [X] T045 [P] Run frontend verification (`npm ci && npm run build && npm test`, verifying `tsc --noEmit` passes with zero errors and checking `package.json` / Vite bundle contains zero WebGL/Three.js packages) in `package.json` and `src/`
- [X] T046 Audit `src-tauri/` and `src/` against Constitution Principles I–V (verify zero writes outside `app_local_data_dir`, zero `File::open` calls bypassing `attr_guard`, zero WebGL/Three.js imports in M1, and alignment with `contracts/tauri-ipc.md`)
- [ ] T047 On native Windows 10/11 (`x86_64-pc-windows-msvc`), run `cargo check --locked` and `cargo test --locked` for `meshcatalog-app` before the Tauri build and `quickstart.md` Scenarios A, B, and C. Verify native IPC, NTFS junction exclusion, 0 bytes downloaded from authorized `Online-Only` fixtures, `< 60s` first scan on ~40 GB SSD, `< 5s` unchanged rescan, `< 100 ms` search and view-switch, `< 16 ms` scrolling, and read-only integrity. Record results in `specs/001-core-local-indexer/quickstart.md`.

**Evidenza:** [gate WSL2 e audit I–V](./quickstart.md#evidenza-wsl2--t001t046). La shell Linux ha superato `cargo check` e i test di servizio. T013, T021 e T032 attendono il [gate Windows T047](./quickstart.md#t047--aperto-verifica-parziale). T039 è verificato con IPC simulato, non nativo.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup completion (`T001`–`T006`) — BLOCKS all user stories.
- **User Story 1 (Phase 3, P1 — MVP)**: Depends on Foundational completion (`T007`–`T013`).
- **User Story 2 (Phase 4, P1)**: Depends on US1 scanner/pipeline foundation (`T015`–`T021`) so exclusions, walker, writer, and Pass 1/Pass 2 pipeline exist.
- **User Story 4 (Phase 5, P1)**: Backend queries (`T030`–`T032`) depend on Foundational DB schema (`T007`, `T010`) and US2 populated folder aggregates/ZIP entries (`T028`); Frontend components (`T033`–`T038`) depend on Foundational TS types (`T012`) and US1 sidebar/topbar baseline (`T022`).
- **User Story 3 (Phase 6, P2)**: Depends on US2 `writer.rs` & `pipeline/mod.rs` (`T028`–`T029`).
- **Polish (Phase 7)**: Depends on all User Stories (`US1`, `US2`, `US4`, `US3`) being complete.

### User Story Dependencies

- **US1 (P1)**: Independent MVP — source CRUD + read-only filesystem guarantee + Win32 Anti-Hydration Guard + Pass 1 cataloging.
- **US2 (P1)**: Extends Pass 1/2 with deterministic pack grouping, technical subfolder absorption, `display_title` cleanup, `.zip` Central Directory reading, and recursive folder aggregates.
- **US4 (P1)**: Consumes US1/US2 SQLite catalog to deliver the 4-area virtualized desktop browser, `Modelli / Pack` ↔ `Singoli File` toggle, `< 100 ms` recursive FTS5 search, and read-only text Inspector.
- **US3 (P2)**: Enhances Pass 2 with `(size_bytes, mtime_unix)` incremental skip, `BLAKE3` hashing, `fingerprint` move/rename reconciliation, and orphan pruning.

### Parallel Opportunities

- **Phase 1**: `T002`, `T003`, `T004`, `T005`, `T006` can run in parallel right after or alongside `T001`.
- **Phase 2**: `T008`, `T009`, `T012` can run in parallel while `T007` and `T010` are implemented.
- **Phase 3 (US1)**: `T014`, `T015`, `T016` can run in parallel; `T022` (frontend) can run in parallel with `T020`–`T021` (backend) once `T012` is ready.
- **Phase 4 (US2)**: `T023`, `T024`, `T025`, `T026` can all run in parallel before integrating `T027`–`T029`.
- **Phase 5 (US4)**: `T030` (perf tests) runs in parallel with `T033`, `T034`, `T035`, `T036`, `T038` (independent React UI components) once `T012` and `T022` are ready.
- **Phase 6 (US3)**: `T040` (incremental/move tests) and `T041` (`blake3_hasher.rs`) can run in parallel before `T042`–`T043`.
- **Phase 7**: `T044` (WSL2 Rust checks) and `T045` (Vite/TS build checks) run in parallel.

---

## Parallel Example: User Story 2 & User Story 4

```bash
# Parallel User Story 2 core modules & test suites:
Task: "T023 [P] [US2] Write grouping & technical subfolder tests in src-tauri/crates/meshcatalog-core/tests/grouping_and_exclusions_tests.rs"
Task: "T024 [P] [US2] Write .zip Central Directory & error recovery tests in src-tauri/crates/meshcatalog-core/tests/zip_and_incremental_tests.rs"
Task: "T025 [P] [US2] Implement display_title cleaner in src-tauri/crates/meshcatalog-core/src/scanner/title.rs"
Task: "T026 [P] [US2] Implement .zip Central Directory reader in src-tauri/crates/meshcatalog-core/src/archive/zip_reader.rs"

# Parallel User Story 4 frontend components & FTS5 benchmark:
Task: "T030 [P] [US4] Write 50k+ FTS5 & subtree query benchmark in src-tauri/crates/meshcatalog-core/tests/fts5_perf_50k_tests.rs"
Task: "T035 [P] [US4] Create FolderCard in src/components/FolderCard.tsx"
Task: "T036 [P] [US4] Create ModelOrFileCard in src/components/ModelOrFileCard.tsx"
Task: "T038 [P] [US4] Implement TextInspectorPanel in src/components/TextInspectorPanel.tsx"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete **Phase 1: Setup** (`T001`–`T006`).
2. Complete **Phase 2: Foundational** (`T007`–`T013`).
3. Complete **Phase 3: User Story 1** (`T014`–`T022`).
4. **STOP and VALIDATE**: Run `cargo test -p meshcatalog-core --test anti_hydration_tests` in WSL2 and confirm read-only user filesystem + zero-hydration guard behavior.

### Incremental Delivery (M1 Complete)

1. **Setup + Foundational** (`T001`–`T013`) → Tauri v2 + SQLite `WAL`/`FTS5` foundation ready.
2. **Add US1** (`T014`–`T022`) → Safe read-only source management & Anti-Hydration Pass 1 (MVP).
3. **Add US2** (`T023`–`T029`) → `.zip` Central Directory inspection, `folder_pack` grouping, clean titles, recursive folder stats.
4. **Add US4** (`T030`–`T039`) → Full 4-area virtualized desktop UI, `Modelli / Pack` ↔ `Singoli File` toggle, `< 100 ms` FTS5 search, text Inspector.
5. **Add US3** (`T040`–`T043`) → Incremental `(size_bytes, mtime_unix)` skip, `BLAKE3` hashing, move/rename resilience.
6. **Polish & Cross-OS Verify** (`T044`–`T047`) → WSL2 CI gate + native Windows `x86_64-pc-windows-msvc` `quickstart.md` verification.

---

## Notes

- `[P]` tasks target distinct files with no unmet dependencies.
- All pure-Rust core logic and unit/integration tests live in `src-tauri/crates/meshcatalog-core` so they execute directly in WSL2 (`cargo test -p meshcatalog-core`) without GUI/WebView2 dependencies.
- Native Windows `x86_64-pc-windows-msvc` testing (`T047`) validates real NTFS attributes and reparse tags from Win32 metadata lookups and `tauri-plugin-dialog` UX per `quickstart.md`.
