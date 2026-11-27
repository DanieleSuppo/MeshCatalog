---

description: "Actionable, dependency-ordered task list for Milestone M2: Pipeline Grafica, Miniature e Viewer 3D On-Demand (002-thumbnails-3d-viewer)"
---

# Tasks: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Input**: Design documents from `/specs/002-thumbnails-3d-viewer/`  
**Prerequisites**: `plan.md` (required), `spec.md` (required), `data-model.md`, `contracts/tauri-ipc.md`, `research.md`, `quickstart.md`, `.specify/memory/constitution.md`

**Tests**: Included explicitly for `meshcatalog-core` (pure-Rust unit & integration tests verifiable on WSL2 Linux via `cargo test` using `MockThumbnailRenderer` and `MockMetadataInspector`) and frontend verification (`tsc --noEmit`, Vite lazy-chunk build, and `node --test` + `jsdom`) per `plan.md` and `quickstart.md`.

**Organization**: Tasks are grouped into Setup, Foundational prerequisites, User Stories (`US1`–`US4`, all Priority P1 in `spec.md`), and Polish & Cross-OS Verification.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: User story mapping (`[US1]`, `[US2]`, `[US3]`, `[US4]`; omitted in Setup, Foundational, and Polish phases)
- Exact file paths are specified in every task description.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add M2 open-source Rust/TypeScript dependencies, configure scoped `assetProtocol` security for `%LOCALAPPDATA%\MeshCatalog\cache\thumbs`, and add M2 UI theme tokens.

- [ ] T001 Add M2 open-source Rust dependencies (`image` with `default-features = false` and `features = ["jpeg", "png", "webp"]`, `quick-xml`, `tobj`, `gltf` with `default-features = false`, and `wgpu`) to `src-tauri/crates/meshcatalog-core/Cargo.toml` and update `src-tauri/Cargo.lock`
- [ ] T002 [P] Add React 19-compatible 3D viewer dependencies (`three` `^0.170`, `@react-three/fiber` `^9`, `@react-three/drei` `^10`, `@types/three`) in `package.json` and configure Vite chunking so Three.js loads in a separate lazy chunk in `vite.config.ts`
- [ ] T003 [P] Enable `app.security.assetProtocol` (`enable: true`, `scope: ["$APPLOCALDATA/cache/thumbs/**"]`) and CSP `img-src 'self' asset: http://asset.localhost data: blob:` (never adding user or Dropbox directories) in `src-tauri/tauri.conf.json` and update `src-tauri/capabilities/default.json` if required by Tauri v2 ACLs
- [ ] T004 [P] Extend studio theme CSS tokens and layout styles (thumbnail containers, 4-angle *Hover Scrub* `--scrub-frame` dots, Pause/Resume scan controls, Inspector 2D/3D header switcher, and `QuickLookModal` overlay) in `src/styles/theme.css`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Additive SQLite migration `004` (`user_version = 4`), M2 `IpcErrorCode` & DTO contracts (Rust + TypeScript), `attr_guard::open_resident` disk-read enforcement, `%LOCALAPPDATA%\MeshCatalog\cache\thumbs` bootstrap, and `get_thumbs_root` IPC wiring that MUST be complete before any User Story implementation begins.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T005 Author additive SQLite migration `004_m2_thumbnails_and_geometry.sql` in `src-tauri/crates/meshcatalog-core/migrations/004_m2_thumbnails_and_geometry.sql` adding `preview_state TEXT NOT NULL DEFAULT 'pending' CHECK (preview_state IN ('pending', 'extracted', 'not_applicable', 'skipped', 'failed'))`, `geometry_state TEXT NOT NULL DEFAULT 'pending' CHECK (geometry_state IN ('pending', 'done', 'skipped_size', 'skipped_triangles', 'skipped_no_adapter', 'failed', 'not_applicable'))`, `pass3_error TEXT`, updating `WHERE storage_state = 'cloud_placeholder'` to `preview_state = 'not_applicable', geometry_state = 'not_applicable', index_pass = 3`, creating partial indexes `idx_files_pass2_pending` and `idx_files_pass3_pending` (`WHERE storage_state IN ('local_resident', 'zip_entry')`), and creating `file_references` (`id`, `file_id REFERENCES files(id) ON DELETE CASCADE`, `ord`, `ref_path`, `ref_kind TEXT NOT NULL DEFAULT 'lyt_stl'`, `UNIQUE(file_id, ord)` + `idx_file_references_file`); register migration `004` (`user_version = 4`) in `src-tauri/crates/meshcatalog-core/src/db/migrations.rs`
- [ ] T006 [P] Extend `IpcErrorCode` and `IpcError` in `src-tauri/crates/meshcatalog-core/src/error.rs` with M2 variants (`'FILE_NOT_FOUND'`, `'CLOUD_PLACEHOLDER_BLOCKED'`, `'UNSUPPORTED_VIEWER_FORMAT'`, `'CONFIRMATION_REQUIRED'`, `'MESH_TOO_LARGE_BLOCKED'`, `'CORRUPT_MESH_OR_ASSET'`) and optional `details: Option<String>` serialized in `IpcError` JSON payloads for `CONFIRMATION_REQUIRED` (`{"sizeBytes":...,"triangleCount":...,"reason":"..."}`)
- [ ] T007 [P] Extend `serde(rename_all = "camelCase")` DTOs in `src-tauri/crates/meshcatalog-core/src/contracts.rs` with `ThumbnailOrigin` (`'creator_image' | '3mf_plate' | 'lys_preview' | 'wgpu_render' | 'dropbox_thumb'`), `ThumbnailDto` (`id`, `relWebpPath`, `origin`, `frameCount: 1 | 4`, `width`, `height`), `GeometryMetricsDto` (`triangleCount`, `bboxXMm`, `bboxYMm`, `bboxZMm`), `ModelCardDto.heroThumbnail`, `FileCardDto` (`thumbnail`, `geometry`, `lytReferences?: string[]`), `ModelDetailDto.heroThumbnail`, unify `ScanPass` with `IndexerPass` (`1 | 2 | 3`) on `ScanStatusDto.pass` alongside `isPaused: bool`, and extend `indexer://progress` with `isPaused: Option<bool>`
- [ ] T008 [P] Extend TypeScript DTOs (`ThumbnailOrigin`, `ThumbnailDto`, `GeometryMetricsDto`, `ViewerMeshData`, `ModelCardDto`, `FileCardDto`, `ModelDetailDto`, `ScanStatusDto` with `isPaused: boolean`, `pass: 1 | 2 | 3 | null`, `IndexerProgressEvent` with `isPaused?: boolean`, `IpcErrorCode`, keeping the existing `IpcError` interface with `details?: string`) in `src/types/catalog.ts`, add `snake_case` IPC helpers matching M1 conventions (`get_thumbs_root`, `pause_index_scan`, `resume_index_scan`, `load_mesh_bytes_for_viewer`, `load_image_for_viewer`) and `MSH1` binary framing parser (`28`-byte header + `Float32Array` `positions`/`normals`) in `src/api/tauriClient.ts`, and update error mapping in `src/api/useCatalogRead.ts`
- [ ] T009 Export `pub mod mesh;` and `pub mod thumbs;` in `src-tauri/crates/meshcatalog-core/src/lib.rs`; add public `attr_guard::open_resident_checked(path, expected_size, expected_mtime, inspector) -> Result<fs::File, IpcError>` in `src-tauri/crates/meshcatalog-core/src/scanner/attr_guard.rs` verifying ancestor + entry Win32 attributes (`is_placeholder()` / `!allows_resident_access()` -> `IpcError::CloudPlaceholderBlocked`, `NotFound` -> `IpcError::FileNotFound`, size/mtime drift -> `IpcError::CorruptMeshOrAsset`); and update `write_inventory_batch`, `cached_analysis` (`f.index_pass >= 2` instead of `= 2`), and `write_analysis` (`index_pass = max(index_pass, 2)` plus `zip_entry` `INSERT` and `ON CONFLICT(uri) DO UPDATE`) in `src-tauri/crates/meshcatalog-core/src/db/writer.rs` so `cloud_placeholder` rows set `preview_state = 'not_applicable', geometry_state = 'not_applicable', index_pass = 3` while new or metadata-changed `local_resident`/`zip_entry` rows reset `index_pass`, `preview_state`/`geometry_state` (by extension), `pass3_error = NULL`, `thumb_id = NULL`, `triangle_count = NULL`, `bbox_x_mm = NULL`, `bbox_y_mm = NULL`, `bbox_z_mm = NULL`
- [ ] T010 [P] Add migration `003 -> 004` upgrade tests (verifying preserved M1 `models`/`files`/`FTS5` IDs, `local_resident`/`zip_entry` pending state, `cloud_placeholder` `not_applicable` + `index_pass = 3`, and `file_references` unique constraint) in `src-tauri/crates/meshcatalog-core/tests/foundation_db.rs`, and add M2 DTO/`IpcErrorCode` serialization tests in `src-tauri/crates/meshcatalog-core/tests/foundation_contracts.rs`
- [ ] T011 Initialize `%LOCALAPPDATA%\MeshCatalog\cache\thumbs` at startup in `src-tauri/src/lib.rs` and implement the `get_thumbs_root` Tauri command returning its canonical path in `src-tauri/src/commands.rs`

**Checkpoint**: Foundation ready — `catalog.db` migrates cleanly from v3 to v4, M2 Rust/TS contracts & `IpcErrorCode` align with `contracts/tauri-ipc.md`, `attr_guard::open_resident_checked` protects all disk reads, and `get_thumbs_root` serves the scoped cache root.

---

## Phase 3: User Story 1 - Estrazione automatica delle anteprime integrate da cartelle, `.zip`, `.3mf` e `.lys`/`.lyt` (Priority: P1) 🎯 MVP

**Goal**: Automatically detect and extract creator images (`.jpg`, `.jpeg`, `.png`, `.webp` inside `folder_pack`, resident `.zip`, or standalone `model_id = NULL`, filtering banners/logos/tiny icons), `.3mf` `Metadata/plate_*.png`, `.lys` `preview.png` (Tier 1 clean-room `<= 32 MB`), and `.lyt` `.stl` references (`file_references`) into `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\<ab>\<hash>.webp` (`256x256`) with `0` bytes downloaded from `cloud_placeholder`s and `0` files written to user directories.

**Independent Test**: Run `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --test m2_preview_extractors_tests` in WSL2 to verify banner/logo filtering, `.3mf` plate & `.lys`/`.lyt` extraction on disk and inside `.zip`, `creator_image > 3mf_plate > lys_preview` hero priority, `.webp` cache deduplication under `cache/thumbs/<ab>/<hash>.webp`, and `0`-byte anti-hydration checks on `cloud_placeholder`s.

### Tests for User Story 1 (WSL2 Pure-Rust Verification)

- [ ] T012 [P] [US1] Write unit & integration tests in `src-tauri/crates/meshcatalog-core/tests/m2_preview_extractors_tests.rs` covering: creator image extraction (`.jpg`/`.jpeg`/`.png`/`.webp`) from folders, resident `.zip` archives, and standalone `model_id = NULL` images; heuristic exclusion of banner/logo/icon tokens (`logo`, `banner`, `icon`, `watermark`, `avatar`, `qr`, `patreon`, `socials`, `badge`), `< 48 px` images, and aspect ratios `> 4.5` or `< 0.22` (while falling back cleanly if no better source exists); `.3mf` `Metadata/plate_1.png` extraction (preferred over `plate_2.png`) from disk and `.zip`; `.lys` Tier 1 `preview.png` extraction (ZIP, JSON/base64, PNG-chunk bounded `<= 32 MB`) and `.lyt` `.stl` reference extraction into `file_references` without opening external `.stl` paths; `creator_image > 3mf_plate > lys_preview` `hero_thumb_id` priority; `.webp` `256x256` deduplication under `cache/thumbs/<ab>/<hash>.webp`; and `0`-byte anti-hydration assertions via `MockMetadataInspector` for `cloud_placeholder` images, `.3mf`, `.lys`, `.lyt`, and `.zip`

### Implementation for User Story 1

- [ ] T013 [P] [US1] Implement thumbnail cache manager in `src-tauri/crates/meshcatalog-core/src/thumbs/mod.rs` supporting deterministic `cache_key` (`img:<blake3>`, `3mf_plate:<blake3>`, `lys_preview:<blake3>`, `wgpu_v1:<blake3>`), `<ab>/<hash>.webp` relative paths under `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\`, pre-decode/pre-render cache hit reuse, `256x256` resize + `.webp` encoding (`frame_count = 1`, `width = 256`, `height = 256`), `1024x256` RGBA->WebP encoding helper (`frame_count = 4`, `width = 1024`, `height = 256`), and unreferenced thumbnail file pruning
- [ ] T014 [P] [US1] Implement Pass 2 extractors in `src-tauri/crates/meshcatalog-core/src/thumbs/extractors.rs` reading strictly in RAM via `attr_guard::open_resident_checked` and `zip::ZipArchive`: creator image classifier/decoder (token + `< 48 px` + aspect ratio `> 4.5` / `< 0.22` filter producing `'extracted' | 'skipped' | 'failed'`), `.3mf` `Metadata/plate_*.png` extractor prioritizing `Metadata/plate_1.png`, `.lys` clean-room `preview.png` extractor (`<= 32 MB` read cap, PNG-validated), and `.lyt` `.stl`/`.obj`/`.3mf` ordered reference parser (`ref_kind IN ('lyt_stl', 'lyt_obj', 'lyt_3mf')`)
- [ ] T015 [US1] Extend `src-tauri/crates/meshcatalog-core/src/db/writer.rs` with Pass 2 persistence: deduplicated `thumbnails` upsert by `cache_key` (`origin IN ('creator_image', '3mf_plate', 'lys_preview')`), updating `files.thumb_id`, `files.preview_state` (`'pending' | 'extracted' | 'not_applicable' | 'skipped' | 'failed'`), `file_references` (`ord`, `ref_path`, `ref_kind`), deterministic `models.hero_thumb_id` selection (`creator_image` max `width * height` tie-break `rel_path` > `3mf_plate` `plate_1.png` priority > `lys_preview`), and preserving `hero_thumb_id = coalesce(destination.hero_thumb_id, models.hero_thumb_id)` in `reconcile_move`
- [ ] T016 [US1] Integrate Pass 2 preview extraction (`creator_image`, `.3mf` plate, `.lys`, `.lyt`) into `src-tauri/crates/meshcatalog-core/src/pipeline/mod.rs` executing *after* `finish_scan` (after move reconciliation and pruning) for `local_resident` and `zip_entry` rows with `preview_state = 'pending'`, ensuring `cloud_placeholder` records remain untouched (`0` bytes opened) and emitting `indexer://progress` / `indexer://batch_updated` / `indexer://error`
- [ ] T017 [US1] Extend `src-tauri/crates/meshcatalog-core/src/db/queries.rs` to join `thumbnails` into `ModelCardDto.heroThumbnail`, `ModelDetailDto.heroThumbnail`, and `FileCardDto.thumbnail` (including `model_id = NULL` images in `Singoli File`), and populate `FileCardDto.lytReferences` only inside `get_model_detail` (omitting it in `search_catalog`)

**Checkpoint**: User Story 1 (MVP) is complete and independently testable — resident creator images, `.3mf` plate renders, `.lys` previews, and `.lyt` references are extracted into `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\` with deterministic hero selection and 0 bytes touched on `cloud_placeholder`s or user folders.

---

## Phase 4: User Story 2 - Analisi geometrica e rendering multi-angolo in background per modelli 3D residenti (Priority: P1)

**Goal**: Parse `.stl` (binary & ASCII), `.obj`, `.ply`, `.glb` (`m -> mm`), and `.3mf` (`3D/3dmodel.model` & `3D/Objects/*.model` `unit` -> `mm` + component/build transforms) into `NormalizedMesh`, persist `triangle_count` and `bbox_x/y/z_mm`, execute two-phase Pass 3 (`3A` uncovered pack heroes / `single_file` -> `3B` remaining pack meshes) with `MeshThumbnailRenderer` (`WgpuThumbnailRenderer` 4-angle `1024x256` `.webp` clay studio + `MockThumbnailRenderer` in WSL2), skip `> 100 MB` (`104_857_600` bytes) or `> 3_000_000` triangles into terminal states (`'skipped_size'`, `'skipped_triangles'`, `'skipped_no_adapter'`, `'failed'`), auto-resume pending Pass 2/3 at startup without repeating Pass 1, and preserve cached `.webp` assets across moves/renames.

**Independent Test**: Run `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --test m2_mesh_parsers_and_hero_tests` and `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --test m2_pass3_queue_resume_pause_tests` in WSL2.

### Tests for User Story 2 (WSL2 Pure-Rust Verification)

- [ ] T018 [P] [US2] Write pure-Rust unit & integration tests in `src-tauri/crates/meshcatalog-core/tests/m2_mesh_parsers_and_hero_tests.rs` verifying: `.stl` (binary & ASCII, including pre-allocation header/size rejection on truncated or oversized counts), `.obj` (N-gon fan triangulation & negative indices), `.ply` (ASCII, binary LE & BE), `.glb` (`m -> mm` scaling + node hierarchy transforms), `.3mf` (`unit` -> `mm` + `3D/Objects/*.model` + `<component>`/`<build><item>` affine transforms), `triangle_count` & `bbox_x/y/z_mm` accuracy, hero selection (`triangle_count * sqrt(bbox_x^2 + bbox_y^2 + bbox_z^2)` skipping `< 2_000` triangle parts when `>= 2_000` candidates exist), heterogeneous valid corpus preview coverage `>= 95%` (`SC-003`), and corrupt/zero-triangle/degenerate-bbox failure handling
- [ ] T019 [P] [US2] Write pure-Rust integration tests in `src-tauri/crates/meshcatalog-core/tests/m2_pass3_queue_resume_pause_tests.rs` using `MockThumbnailRenderer` verifying: two-phase Pass 3 queue ordering (Phase 3A uncovered pack hero + `single_file` before Phase 3B internal pack meshes, plus fallback to next pack piece when candidate fails or exceeds `3_000_000` triangles); `> 104_857_600` bytes -> `geometry_state = 'skipped_size'`, `> 3_000_000` triangles -> `'skipped_triangles'`, `RenderError::NoAdapter` -> `'skipped_no_adapter'`, corrupt -> `'failed'` (all `index_pass = 3` with `pass3_error`); startup auto-resume (`run_pending_passes`) without re-running Pass 1 and without re-queuing terminal rows; `PauseGate` pause/resume/cancel; move/rename cache preservation (`cache_key` reuse without re-render + safe orphan pruning); and `0`-byte skip on `cloud_placeholder`s

### Implementation for User Story 2

- [ ] T020 [P] [US2] Implement `NormalizedMesh`, `triangle_count` & `bbox_mm` calculation, hero score (`triangle_count * bbox_diagonal_mm` avoiding `< 2_000` triangle minor parts when richer meshes exist), `MSH1` binary serializer (`"MSH1"` magic, `version = 1`, `triangleCount`, `bboxXMm`/`YMm`/`ZMm`, `vertexCount`, native-axis `mm` `positions` centered strictly on bounding-box center `(0, 0, 0)` + `normals`), and format dispatcher in `src-tauri/crates/meshcatalog-core/src/mesh/mod.rs`
- [ ] T021 [P] [US2] Implement `.stl` (binary header budget check + ASCII) and `.ply` (ASCII, `binary_little_endian`, `binary_big_endian`, vertex/face count cap + fan triangulation) parsers in `src-tauri/crates/meshcatalog-core/src/mesh/stl.rs` and `src-tauri/crates/meshcatalog-core/src/mesh/ply.rs`
- [ ] T022 [P] [US2] Implement `.obj` (`tobj`/fan triangulation + negative indices), `.glb` (`gltf` node transforms + `m -> mm`), and `.3mf` (`zip` + `quick-xml` `3D/3dmodel.model` & `3D/Objects/*.model` `unit` -> `mm` + `<component>`/`<build><item>` affine transforms) parsers in `src-tauri/crates/meshcatalog-core/src/mesh/obj.rs`, `src-tauri/crates/meshcatalog-core/src/mesh/glb.rs`, and `src-tauri/crates/meshcatalog-core/src/mesh/threemf.rs`
- [ ] T023 [P] [US2] Implement `MeshThumbnailRenderer` trait (`render_four_angles` -> `1024x256` RGBA), `MockThumbnailRenderer` (pure-Rust deterministic 4-panel strip for WSL2 tests), and headless `WgpuThumbnailRenderer` (`HighPerformance` -> `LowPower` -> fallback adapter, `Depth32Float`, studio clay `#B8BDC4` on `#1B1F24`, `0°`/`90°`/`180°`/`270°` at ~`25°` pitch, `512x512` supersampled downsample to `256x256`, `RenderError::NoAdapter`) in `src-tauri/crates/meshcatalog-core/src/thumbs/renderer.rs`
- [ ] T024 [US2] Extend `src-tauri/crates/meshcatalog-core/src/db/writer.rs` with Pass 3 queries & persistence: Phase 3A/3B queue fetch (`WHERE storage_state IN ('local_resident', 'zip_entry') AND geometry_state = 'pending'`), persisting `triangle_count`, `bbox_x_mm`, `bbox_y_mm`, `bbox_z_mm`, `geometry_state`, `pass3_error`, `index_pass = 3`, `wgpu_render` `thumbnails` (`frame_count = 4`, `width = 1024`, `height = 256`), `models.hero_thumb_id` promotion respecting `creator_image > 3mf_plate > lys_preview > wgpu_render`, and post-scan orphan `.webp` cleanup running strictly after a complete non-cancelled scan (`!cancelled && !coverage.incomplete`) verifying `thumbnails.id` is unreferenced by `files.thumb_id`, `models.hero_thumb_id`, and `folders.hero_thumb_id`
- [ ] T025 [US2] Implement two-phase Pass 3 worker executing *after* `finish_scan` (reading files strictly via `attr_guard::open_resident_checked`, enforcing `104_857_600` byte / `3_000_000` triangle skip limits, per-pack candidate fallback, per-source `PauseGate` + `CancellationToken` checks, `indexer://progress` / `indexer://batch_updated` / `indexer://error` emission) and `run_pending_passes` (auto-resuming pending Pass 2 + Pass 3 without repeating Pass 1) in `src-tauri/crates/meshcatalog-core/src/pipeline/mod.rs`
- [ ] T026 [US2] Integrate per-active-source `PauseGate` (waking paused workers immediately on `cancel` and `remove` before waiting on `released` to avoid deadlocks, and computing `ScanStatusDto.isPaused` + unified `IndexerPass` `pass` across active sources), Pass 3 `WgpuThumbnailRenderer`, startup `resume_pending_at_startup` reserving pending sources in `ScanState.active` so `get_scan_status()` stays synchronized before WebView listeners attach, and `pause_index_scan` / `resume_index_scan` / extended `cancel_index_scan` IPC handlers in `src-tauri/src/scanner_service.rs`, `src-tauri/src/commands.rs`, and `src-tauri/src/lib.rs`

**Checkpoint**: User Story 2 is complete — Pass 3 parses all 5 open 3D formats, computes real `mm` dimensions & triangle counts, renders 4-angle `1024x256` `.webp` strips in two priority phases with terminal skip states, auto-resumes at startup, and reuses cached `.webp` files across local moves/renames.

---

## Phase 5: User Story 3 - Griglia visiva istantanea e *Hover Scrub* a 4 angolazioni (Priority: P1)

**Goal**: Render cached `.webp` thumbnails in both `Modelli / Pack` and `Singoli File` (including standalone `model_id = NULL` images) via scoped `assetProtocol` with zero WebGL in grid cards (`< 16 ms` scroll budget), support instant 4-angle CSS *Hover Scrub* (`1024x256`) with immediate reset on `mouseleave`, progressively update visible cards on `indexer://batch_updated` without resetting scroll or search, and expose Pass 1–3 Pause/Resume/Stop controls in `TopCommandBar`.

**Independent Test**: Run `npm run build && npm test` (covering `tests/viewer_and_scrub.test.mjs` US3 scenarios plus existing catalog browser/scroll-reset tests).

### Tests for User Story 3 (Frontend Verification)

- [ ] T027 [P] [US3] Write frontend automated tests in `tests/viewer_and_scrub.test.mjs` (US3 section) and update M1 test fixtures (`tests/fixtures/catalog.mjs` for `get_thumbs_root`, `pause_index_scan`, `resume_index_scan`, `load_mesh_bytes_for_viewer`, `load_image_for_viewer`, `isPaused: false`; `tests/t022_sidebar_topbar.test.mjs` for `isPaused` & Pass 3 pill; keeping `tests/catalog-browser.test.mjs` compatible via `<article>` card interaction or aligned assertions) verifying: safe `convertFileSrc` URL resolution rejecting `..` segments; 4-angle CSS `--scrub-frame` (`0..3`) updates on `pointermove` and instant reset to `0` on `mouseleave` without extra React renders or WebGL contexts (`0` WebGL in grid cards, `< 16 ms` scroll handler budget); `thumbnail` display on standalone `modelId === null` image cards in `Singoli File`; clean format/`Cloud / Online-Only` fallback rendering when `thumbnail === null`; progressive `indexer://batch_updated` refresh preserving grid scroll offset & search query; and `TopCommandBar` Pause/Resume/Stop (`pause_index_scan`, `resume_index_scan`, `cancel_index_scan`) controls for Pass 1–3

### Implementation for User Story 3

- [ ] T028 [P] [US3] Extend `src/components/TopCommandBar.tsx` to display Pass `1 | 2 | 3` (`phaseLabel`, `processed`/`total`, `currentItem`, `isPaused`) and wire **Pausa / Riprendi / Annulla** buttons to `pause_index_scan`, `resume_index_scan`, and `cancel_index_scan`
- [ ] T029 [P] [US3] Extend `src/components/ModelOrFileCard.tsx` and `src/components/CatalogGrid.tsx`: render cached `.webp` thumbnails (`heroThumbnail` / `thumbnail`, including `modelId === null` images in `Singoli File`) via `convertFileSrc(thumbsRoot + '/' + relWebpPath)` with `..` guard, keep `<article>` card interaction (`onClick`/keyboard Enter) for standalone `modelId === null` items (preserving non-`<button>` DOM distinction required by `tests/catalog-browser.test.mjs`), update `CatalogGrid.tsx` virtualizer `rowHeight` for thumbnail cards (`<= 270 px`, keeping M1 `tests/catalog-browser.test.mjs` scroll offset at `27000` inside item `>= 200`, or updating scroll offset cleanly in `T027`) and widen `onInspect: (modelId: number | null, itemId: number) => void`, implement zero-re-render 4-angle *Hover Scrub* (`background-size: 400% 100%`, `background-position-x: calc(var(--scrub-frame, 0) * 33.3333%)` updated on DOM node during `onPointerMove` and reset on `onPointerLeave`/`mouseleave`), keep zero WebGL/3D imports inside grid cards, and preserve clean M1 format/`Cloud / Online-Only` fallback badges
- [ ] T030 [US3] Update `src/App.tsx` to remove the `p.pass === 3 ? 2 : p.pass` clamp in `useIndexer` (`L40`), load `get_thumbs_root()` once at boot, pass `thumbsRoot` to `CatalogGrid` / Inspector / QuickLook, and refresh visible catalog pages on `indexer://batch_updated` without resetting scroll position or active search/filter state

**Checkpoint**: User Story 3 is complete — the virtualized grid displays cached `.webp` thumbnails and 4-angle CSS *Hover Scrub* with `< 16 ms` frame responsiveness, live progressive batch updates, and Pass 1–3 Pause/Resume/Stop controls.

---

## Phase 6: User Story 4 - Viewer 3D interattivo on-demand nell'Inspector e QuickLook (`Spazio`) (Priority: P1)

**Goal**: Provide a single lazy-loaded `@react-three/fiber` v9 `ThreeViewport` mounted on demand inside `InspectorPanel` (which opens in 2D/metadata mode and automatically resets back to 2D on card selection changes) or `QuickLookModal` (`Space` outside text inputs / `Esc`), loading `MSH1` binary meshes via `load_mesh_bytes_for_viewer(fileId, allowRemoteFetch, confirmLarge)` from resident disk or `.zip` in RAM (`<= 100 MB` / `<= 3_000_000` direct, `100–500 MB` or `> 3_000_000` `'CONFIRMATION_REQUIRED'`, `> 500 MB` `'MESH_TOO_LARGE_BLOCKED'`, `cloud_placeholder` `'CLOUD_PLACEHOLDER_BLOCKED'`), supporting pack/`.zip` piece switching, wireframe/grid/axes toggles, 2D preview via `load_image_for_viewer` for images (`modelId === null` included), and explicit WebGL buffer disposal on unmount.

**Independent Test**: Run `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --test m2_viewer_loader_tests` in WSL2 and `npm run build && npm test` (confirming lazy Three.js bundle split + `tests/viewer_and_scrub.test.mjs` US4 tests).

### Tests for User Story 4 (WSL2 Pure-Rust & Frontend Verification)

- [ ] T031 [P] [US4] Write pure-Rust integration tests in `src-tauri/crates/meshcatalog-core/tests/m2_viewer_loader_tests.rs` verifying `load_resident_mesh_msh1` and `load_resident_image_bytes`: valid `MSH1` binary payload (magic, version `1`, `triangleCount`, `bboxXMm`/`YMm`/`ZMm`, `vertexCount`, centered `mm` `positions`/`normals`) from resident disk and `.zip` (`0` temp files written); `cloud_placeholder` returning `'CLOUD_PLACEHOLDER_BLOCKED'` with `0` bytes read; `100–500 MB` (`> 104_857_600` to `<= 524_288_000`) or `> 3_000_000` triangles returning `'CONFIRMATION_REQUIRED'` with structured `details` JSON unless `confirmLarge = true`; `> 524_288_000` bytes returning `'MESH_TOO_LARGE_BLOCKED'`; unsupported extensions returning `'UNSUPPORTED_VIEWER_FORMAT'`; corrupt meshes returning `'CORRUPT_MESH_OR_ASSET'`; and `load_resident_image_bytes` supporting `model_id = NULL` images up to `50 MB` while blocking `cloud_placeholder`
- [ ] T032 [P] [US4] Extend `tests/viewer_and_scrub.test.mjs` (US4 section, ensuring `tests/catalog-browser.test.mjs` and `tests/fixtures/catalog.mjs` stay aligned) verifying: `InspectorPanel` opens in 2D/metadata mode (showing `GeometryMetricsDto`, `lytReferences`, and pack piece list) and resets back to 2D on card switch; `Space` opens/closes `QuickLookModal` when focus is outside `<input>`/`<textarea>`/`<select>`/`[contenteditable]` and inserts text normally inside search inputs; `Esc` closes `QuickLookModal`; opening `QuickLookModal` unmounts any active Inspector `ThreeViewport` (max 1 active WebGL instance); `CONFIRMATION_REQUIRED` dialog -> `confirmLarge: true` retry; `CLOUD_PLACEHOLDER_BLOCKED` notice without mounting WebGL; standalone `modelId === null` image opening 2D QuickLook without calling `get_model_detail`; and explicit `geometry.dispose()` / `material.dispose()` on unmount

### Implementation for User Story 4

- [ ] T033 [P] [US4] Implement core viewer asset loader in `src-tauri/crates/meshcatalog-core/src/mesh/viewer_loader.rs`: `load_resident_mesh_msh1` (checking `storage_state`, enforcing `attr_guard::open_resident_checked`, rejecting `cloud_placeholder`/`dropbox_remote` even when remote fetch is requested, `104_857_600` / `524_288_000` byte limits, `3_000_000` triangle limit, in-memory `.zip` extraction, `MSH1` serialization) and `load_resident_image_bytes` (resident disk / `.zip` image bytes up to `50 MB`, including `model_id = NULL`, blocking `cloud_placeholder`)
- [ ] T034 [US4] Expose `load_mesh_bytes_for_viewer(fileId, allowRemoteFetch, confirmLarge)` (explicitly ignoring/rejecting `allowRemoteFetch` on non-resident files and always returning `IpcError::CloudPlaceholderBlocked` with `0` bytes downloaded in M2) and `load_image_for_viewer(fileId)` returning `tauri::ipc::Response` in `src-tauri/src/commands.rs` and register all M2 commands in `src-tauri/src/lib.rs`
- [ ] T035 [P] [US4] Create lazy-loaded `src/components/ThreeViewport.tsx` (`@react-three/fiber` v9 + `@react-three/drei` v10 + `three`) parsing `MSH1` `ArrayBuffer` into bounding-box-centered `BufferGeometry`, placing the print-bed grid directly under the model at `y = -bboxYMm / 2`, rendering neutral clay/matcap studio lighting with `OrbitControls` + camera reset, independent **wireframe**, **griglia piano di stampa**, and **assi cartesiani** toggles without reloading mesh bytes, and explicit `geometry.dispose()` / `material.dispose()` cleanup on unmount or mesh change
- [ ] T036 [P] [US4] Replace/evolve `src/components/TextInspectorPanel.tsx` into `src/components/InspectorPanel.tsx` showing 2D cached thumbnail + metadata + `GeometryMetricsDto` (`mm` & triangles) + `lytReferences` + pack/`.zip` 3D piece switcher by default, explicit `"Carica vista 3D"` button (`React.lazy` `ThreeViewport`), automatic reset to 2D on card selection change, `CONFIRMATION_REQUIRED` / `MESH_TOO_LARGE_BLOCKED` / `CLOUD_PLACEHOLDER_BLOCKED` handling, and safe 2D image preview when `modelId === null`
- [ ] T037 [US4] Create `src/components/QuickLookModal.tsx` supporting on-demand `ThreeViewport` (`React.lazy`), pack/`.zip` piece selector, 2D preview via `load_image_for_viewer` for image files (including `modelId === null`), safe `Online-Only` message (with cached hero preview if present, `0` bytes downloaded), heavy-mesh `CONFIRMATION_REQUIRED` prompt (`confirmLarge: true`), and `MESH_TOO_LARGE_BLOCKED` error banner
- [ ] T038 [US4] Wire global `Space` (ignoring `<input>`, `<textarea>`, `<select>`, `[contenteditable]`) and `Esc` handlers, mutually exclusive single `ThreeViewport` coordination between `InspectorPanel` and `QuickLookModal`, and `modelId === null` image selection in `src/App.tsx`

**Checkpoint**: User Story 4 is complete — users can inspect any resident mesh or `.zip` piece in 3D on demand inside `InspectorPanel` (2D-first, auto-resetting on card switch) or `QuickLookModal` (`Space` / `Esc`), preview standalone images in 2D, and cleanly dispose WebGL memory on close.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final cross-OS verification (`WSL2` Rust + TypeScript/Vite build + `node --test`, Constitution Principles I–V audit, and native `Windows 10/11` `x86_64-pc-windows-msvc` `wgpu`/WebView2/Three.js/Smart-Sync verification per `quickstart.md`).

- [ ] T039 [P] Run full WSL2 Rust verification (`cargo fmt --check --manifest-path src-tauri/Cargo.toml`, `cargo clippy --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --all-targets -- -D warnings`, `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core`) and fix any warnings or test failures in `src-tauri/crates/meshcatalog-core/`
- [ ] T040 [P] Run full frontend verification (`npm ci && npm run build && npm test`) in `package.json`, `src/`, and `tests/`, confirming zero `tsc --noEmit` errors, isolated lazy Three.js chunk in `dist/assets/`, and all M1 (`catalog-browser.test.mjs`, `catalog-scroll-reset.test.mjs`, `t022_sidebar_topbar.test.mjs`, `tauri_contracts.test.mjs`) + M2 (`viewer_and_scrub.test.mjs`) `node --test` suites passing
- [ ] T041 Audit `src-tauri/` and `src/` against Constitution Principles I–V (verify zero writes outside `%LOCALAPPDATA%\MeshCatalog`, zero `File::open` calls bypassing `attr_guard::open_resident` / `open_resident_checked`, `assetProtocol` strictly scoped to `$APPLOCALDATA/cache/thumbs/**`, zero WebGL in grid cards, max 1 active `ThreeViewport`, and zero proprietary SDK dependencies)
- [ ] T042 On native Windows 10/11 (`x86_64-pc-windows-msvc`), run `cargo check --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --workspace`, `cargo test --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --workspace`, and `npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked`, executing `quickstart.md` Scenarios A, B, and C (real `wgpu` DX12/WARP 4-angle `.webp` rendering, WebView2 `asset://` + `< 16 ms` scroll + *Hover Scrub*, Three.js Inspector & `Space` QuickLook `< 1.5 s` open / `< 1 s` disposal across 20 cycles, Pause/Resume/Auto-Resume, and `0`-byte Dropbox Smart Sync *Online-Only* verification) and record results in `specs/002-thumbnails-3d-viewer/quickstart.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately (`T001`–`T004`).
- **Foundational (Phase 2)**: Depends on Setup (`T001`–`T004`) — BLOCKS all user stories (`T005`–`T011`).
- **User Story 1 (Phase 3, P1 — MVP)**: Depends on Foundational completion (`T005`–`T011`).
- **User Story 2 (Phase 4, P1)**: Depends on Foundational (`T005`–`T011`) and integrates with US1 thumbnail cache & Pass 2/hero priority in `thumbs/mod.rs` (`T013`), `writer.rs` (`T015`), and `pipeline/mod.rs` (`T016`).
- **User Story 3 (Phase 5, P1)**: Frontend tasks (`T027`–`T030`) depend on Foundational TS contracts (`T008`), `assetProtocol` (`T003`), `get_thumbs_root` (`T011`), and backend `ThumbnailDto` / `ScanStatusDto` (`T007`, `T017`, `T026`).
- **User Story 4 (Phase 6, P1)**: Backend loader (`T031`, `T033`, `T034`) depends on `NormalizedMesh` & `MSH1` serializer (`T020`–`T022`) and `IpcErrorCode` (`T006`); Frontend viewer (`T032`, `T035`–`T038`) depends on `T002`, `T008`, `T029`–`T030`.
- **Polish (Phase 7)**: Depends on all User Stories (`US1`–`US4`) being complete.

### User Story Dependencies

- **US1 (P1)**: Independent MVP — extracts native creator images, `.3mf` plates, `.lys` previews, and `.lyt` references into `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\` and `file_references`.
- **US2 (P1)**: Adds 5 open-source 3D mesh parsers, geometry metrics, headless `wgpu` 4-angle rendering, two-phase Pass 3 queue, terminal skip states, and startup auto-resume.
- **US3 (P1)**: Surfaces US1/US2 `.webp` thumbnails and 4-angle *Hover Scrub* in the zero-latency virtualized grid with progressive batch refresh and Pass 1–3 Pause/Resume/Stop controls.
- **US4 (P1)**: Adds on-demand single-instance `@react-three/fiber` v9 `ThreeViewport` in `InspectorPanel` (2D-first, auto-reset on card switch) and `QuickLookModal` (`Space`/`Esc`) backed by `MSH1` binary IPC streaming.

### Parallel Opportunities

- **Phase 1**: `T002`, `T003`, `T004` can run in parallel alongside/right after `T001`.
- **Phase 2**: `T006`, `T007`, `T008`, `T010` can run in parallel once `T005` schema definitions are locked.
- **Phase 3 (US1)**: `T012`, `T013`, `T014` can run in parallel before wiring `T015`–`T017`.
- **Phase 4 (US2)**: `T018`, `T019`, `T020`, `T021`, `T022`, `T023` target distinct files and can run in parallel before integrating `T024`–`T026`.
- **Phase 5 (US3)**: `T027`, `T028`, `T029` can run in parallel before `T030`.
- **Phase 6 (US4)**: `T031`, `T032`, `T033`, `T035`, `T036` can run in parallel before wiring `T034`, `T037`, `T038`.
- **Phase 7**: `T039` (WSL2 Rust checks) and `T040` (Frontend build & `node --test`) run in parallel before `T041`–`T042`.

---

## Parallel Example: User Story 2 & User Story 4

```bash
# Parallel User Story 2 parsers, renderer trait, and test suites:
Task: "T018 [P] [US2] Write mesh parser & hero selection tests in src-tauri/crates/meshcatalog-core/tests/m2_mesh_parsers_and_hero_tests.rs"
Task: "T020 [P] [US2] Implement NormalizedMesh, hero score, and MSH1 serializer in src-tauri/crates/meshcatalog-core/src/mesh/mod.rs"
Task: "T021 [P] [US2] Implement .stl and .ply parsers in src-tauri/crates/meshcatalog-core/src/mesh/stl.rs and src-tauri/crates/meshcatalog-core/src/mesh/ply.rs"
Task: "T022 [P] [US2] Implement .obj, .glb, and .3mf parsers in src-tauri/crates/meshcatalog-core/src/mesh/obj.rs, glb.rs, and threemf.rs"
Task: "T023 [P] [US2] Implement MeshThumbnailRenderer, MockThumbnailRenderer, and WgpuThumbnailRenderer in src-tauri/crates/meshcatalog-core/src/thumbs/renderer.rs"

# Parallel User Story 4 backend loader & frontend viewer components:
Task: "T031 [P] [US4] Write MSH1 & viewer threshold tests in src-tauri/crates/meshcatalog-core/tests/m2_viewer_loader_tests.rs"
Task: "T033 [P] [US4] Implement load_resident_mesh_msh1 and load_resident_image_bytes in src-tauri/crates/meshcatalog-core/src/mesh/viewer_loader.rs"
Task: "T035 [P] [US4] Create lazy ThreeViewport in src/components/ThreeViewport.tsx"
Task: "T036 [P] [US4] Evolve InspectorPanel in src/components/InspectorPanel.tsx"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete **Phase 1: Setup** (`T001`–`T004`).
2. Complete **Phase 2: Foundational** (`T005`–`T011`).
3. Complete **Phase 3: User Story 1** (`T012`–`T017`).
4. **STOP and VALIDATE**: Run `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --test m2_preview_extractors_tests` in WSL2 and confirm read-only extraction, banner filtering, `.lyt` persistence, and `0`-byte anti-hydration behavior.

### Incremental Delivery (M2 Complete)

1. **Setup + Foundational** (`T001`–`T011`) → Migration `004`, M2 IPC contracts, scoped `assetProtocol`, and `attr_guard::open_resident_checked` ready.
2. **Add US1** (`T012`–`T017`) → Pass 2 native preview extraction & `.lyt` reference catalog (MVP).
3. **Add US2** (`T018`–`T026`) → 5 mesh parsers, headless `wgpu` 4-angle rendering, two-phase Pass 3, terminal states, and startup auto-resume.
4. **Add US3** (`T027`–`T030`) → Zero-latency visual grid, 4-angle CSS *Hover Scrub*, progressive `indexer://batch_updated`, and Pause/Resume/Stop controls.
5. **Add US4** (`T031`–`T038`) → Single lazy `@react-three/fiber` v9 `ThreeViewport`, `MSH1` streaming IPC, 2D-first `InspectorPanel`, and `Space`/`Esc` `QuickLookModal`.
6. **Polish & Cross-OS Verify** (`T039`–`T042`) → WSL2 CI gate + native Windows `x86_64-pc-windows-msvc` `quickstart.md` verification.

---

## Notes

- `[P]` tasks target distinct files with no incomplete dependencies.
- Pure-Rust core logic and unit/integration tests live in `src-tauri/crates/meshcatalog-core` (`MockThumbnailRenderer` + `MockMetadataInspector`) so the entire Pass 2/3 & `MSH1` pipeline runs deterministically in WSL2 without GPU hardware.
- Native Windows `x86_64-pc-windows-msvc` verification (`T042`) validates real `wgpu` DX12/WARP rendering, WebView2 `asset://` + WebGL2 disposal, and Dropbox Smart Sync *Online-Only* zero-hydration behavior per `quickstart.md`.
