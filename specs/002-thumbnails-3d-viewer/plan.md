# Implementation Plan: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Branch**: `002-thumbnails-3d-viewer` | **Date**: 2026-11-27 | **Spec**: [spec.md](./spec.md)  
**Input**: Feature specification from [specs/002-thumbnails-3d-viewer/spec.md](./spec.md)

## Summary

La Milestone M2 completa la pipeline grafica locale di **MeshCatalog** in continuità diretta con il core M1 (`001-core-local-indexer`), mantenendo rigorosamente l'invariante di sola lettura e l'anti-idratazione Win32:
- **Passata 2 (Estrazione Anteprime Native & Tier 1):** Riconoscimento ed estrazione in RAM (senza scrivere o estrarre file nelle cartelle utente) di immagini creator `.jpg`/`.jpeg`/`.png`/`.webp` (con filtro euristico anti-banner/loghi e supporto alle immagini non associate `model_id = NULL`), render di piastra `Metadata/plate_*.png` dai `.3mf`, anteprime `preview.png` dai `.lys` senza SDK proprietari e riferimenti `.stl` dai `.lyt` (persistiti in `file_references`), salvati come `.webp` `256×256` deduplicati per `cache_key` esclusivamente sotto `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\<ab>\<hash>.webp`.
- **Passata 3 (Analisi Geometrica & Rendering Headless `wgpu` a Due Fasi):** Parsing open-source di `.stl` (binario e ASCII), `.obj`, `.ply`, `.glb` (`m -> mm`) e `.3mf` (`unit` -> `mm` + trasformazioni componenti) per calcolo di `triangle_count`, `bbox_x/y/z_mm` e selezione del pezzo principale (`triangoli × diagonale`, evitando micro-pezzi `< 2.000` triangoli). Rendering headless multi-angolo a 4 viste (`1024×256` `.webp` clay studio) eseguito prima sulle copertine mancanti / `single_file` (Fase 3A) e poi sulle restanti mesh dei pack (Fase 3B), saltando in stato terminale file `> 100 MB` o `> 3M` triangoli. Ripresa automatica all'avvio per record pendenti e controlli UI **Pausa / Riprendi / Annulla**.
- **Griglia Zero-Latency & Singolo Viewer 3D On-Demand:** Servizio sicuro delle miniature tramite `assetProtocol` Tauri v2 limitato al solo `$APPLOCALDATA/cache/thumbs/**`, card con *Hover Scrub* a 4 angolazioni a zero WebGL (`< 16 ms` per fotogramma) e singola istanza lazy-loaded di `Three.js` (`@react-three/fiber` v9 + `@react-three/drei` v10) montata su richiesta nell'Inspector (sempre aperto in 2D al cambio scheda) o nel **QuickLook** (`Spazio` / `Esc`), con caricamento binario `MSH1` da disco o `.zip` residente, soglie `100 MB / 500 MB / 3M triangoli`, anteprima 2D per immagini e blocco sicuro su *Online-Only*.

---

## Technical Context

**Language/Version**: Rust (stable, Edition 2021) per `meshcatalog-core` e `meshcatalog-app`; TypeScript `~5.9` + React `19.0` per frontend (`Node.js 22 LTS >= 22.12.0`).  
**Primary Dependencies**:
- **Backend (`src-tauri/crates/meshcatalog-core` + `src-tauri`):** `tauri` v2 (`assetProtocol`), `rusqlite` (`bundled`, `WAL`, `FTS5`), `blake3`, `zip`, `image` (MIT/Apache-2.0, feature `jpeg`, `png`, `webp` per decoding e encoding `.webp` pure-Rust; opzionalmente `webp` / `libwebp` BSD-3-Clause valutato in [research.md](./research.md)), `quick-xml` (MIT/Apache-2.0 per `.3mf`), `tobj` (MIT per `.obj`), `gltf` (MIT/Apache-2.0 per `.glb`), parser pure-Rust dedicati `.stl`/`.ply`/`.lys`/`.lyt`, `wgpu` (MIT/Apache-2.0 per rendering headless off-screen in `meshcatalog-core` dietro trait `MeshThumbnailRenderer` testabile con `MockThumbnailRenderer`), `tokio`, `tokio-util`, `rayon`, `serde`, `serde_json`, `thiserror`.
- **Frontend (`src/`):** `react` 19, `react-dom` 19, `@tanstack/react-virtual`, `@tauri-apps/api` v2 (`convertFileSrc`, `invoke`, `listen`), `three` (`^0.170`), `@react-three/fiber` (`^9.x` compatibile React 19), `@react-three/drei` (`^10.x`), `@types/three`, Vite 6.  
**Storage**: SQLite locale (`%LOCALAPPDATA%\MeshCatalog\catalog.db`, migrazione additiva `004_m2_thumbnails_and_geometry.sql` `user_version = 4`) + cache miniature `.webp` esclusiva in `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\<ab>\<hash>.webp`. Zero scritture o file temporanei nelle cartelle utente.  
**Testing**:
- **WSL2 (Linux):** `cargo fmt --check`, `cargo clippy --locked -p meshcatalog-core --all-targets -- -D warnings`, `cargo test --locked -p meshcatalog-core` (unit & integration test per estrattori `.3mf`/`.lys`/`.lyt`, filtro banner, parser mesh `.stl`/`.obj`/`.ply`/`.glb`/`.3mf`, punteggio Hero, coda Passata 3 a due fasi, stati terminali anti-loop, deduplicazione cache, `MockThumbnailRenderer`, `attr_guard`), più `npm run build` (`tsc --noEmit` + chunking lazy Vite) e `npm test` (`node --test` + `jsdom`).
- **Windows 10/11 (`x86_64-pc-windows-msvc`):** `cargo check --locked --workspace`, `cargo test --locked --workspace` (inclusi `wgpu` headless adapter/WARP fallback e `ScannerService` pausa/riprendi/auto-resume), più verifica interattiva `cargo tauri dev` su WebView2 con GPU reale e segnaposto Dropbox *Online-Only*.  
**Target Platform**: Desktop Windows 10/11 (`x86_64-pc-windows-msvc`), sviluppato cross-OS da WSL2 Linux via GitHub (`DanieleSuppo/MeshCatalog`).  
**Project Type**: Desktop Application (`Tauri v2`: Cargo workspace Rust + SPA React/TypeScript Vite).  
**Performance Goals**:
- Griglia virtualizzata a `10.000+` card con miniature e *Hover Scrub* a 4 viste: `< 16 ms` per fotogramma (SC-004).
- Apertura file 3D residente standard (`<= 50 MB` / `<= 1M` triangoli, da disco o `.zip` residente) in Inspector/QuickLook: `< 1,5 s`; rilascio memoria grafica alla chiusura entro `< 1 s` senza leak su 20 cicli (SC-005).
- Riuso miniature su spostamenti/rinomine tramite impronta: `100%` senza ri-rendering (SC-006).  
**Constraints**:
- **0 byte** scaricati da file o cartelle `cloud_placeholder` in Passata 2, Passata 3, griglia e Viewer/QuickLook (`attr_guard::open_resident` obbligatorio su ogni lettura disco).
- **0 file** creati, modificati o estratti nelle directory dell'utente (tutto lo streaming `.zip`/`.3mf` avviene in RAM limitata).
- Soglie sicurezza mesh: Passata 3 salta `> 100 MB` o `> 3M` triangoli con stato terminale non ripetitivo; Viewer carica diretto `<= 100 MB` / `<= 3M` triangoli, chiede conferma esplicita (`confirmLarge`) tra `100–500 MB` (o `> 3M` entro `500 MB`) e blocca `> 500 MB`.
- Massimo **1** contesto WebGL attivo nell'intera applicazione, montato solo on-demand via `React.lazy`.  
**Scale/Scope**: Progettato per collezioni di `50.000–200.000+` file (mix `~40 GB` SSD locale + centinaia di GB *Online-Only*), senza degradare i tempi di avvio o lo scroll UI.

---

## Constitution Check

*GATE: Verifica Pre-Design (Phase 0) e Post-Design (Phase 1) rispetto a [MeshCatalog Constitution](../../.specify/memory/constitution.md).*

| Principio | Stato Pre-Design | Stato Post-Design | Verifica e Controlli Specifici in M2 |
| :--- | :---: | :---: | :--- |
| **I. Read-Only User Filesystem (Zero Sidecars)** | PASS | PASS | Nessuna estrazione `.zip`/`.3mf` su disco utente; lettura entry in RAM limitata; miniature `.webp` e `catalog.db` salvati esclusivamente in `%LOCALAPPDATA%\MeshCatalog\`. `assetProtocol` limitato rigorosamente a `$APPLOCALDATA/cache/thumbs/**`. |
| **II. Dropbox Zero-Explosion (Win32 Anti-Hydration Guard)** | PASS | PASS | Ogni apertura file in Passata 2, Passata 3, `load_mesh_bytes_for_viewer` e `load_image_for_viewer` passa da `attr_guard::open_resident_checked`. I record `cloud_placeholder` vengono marcati `not_applicable` nella migrazione `004` e in `write_inventory_batch`, non entrano nelle code background e sono bloccati in IPC con `CLOUD_PLACEHOLDER_BLOCKED` (anche se `allowRemoteFetch` è impostato; `0` byte scaricati). |
| **III. Zero-Latency 2D Browsing vs On-Demand 3D Viewport** | PASS | PASS | Griglia e *Hover Scrub* a 4 angolazioni leggono solo SQLite (`WAL`) e file `.webp` locali via CSS `background-position` senza WebGL né parser mesh nelle card (`< 16 ms`). Un unico componente `ThreeViewport` (`React.lazy`) è montato on-demand nell'Inspector (che torna sempre in 2D al cambio scheda) o nel QuickLook (`Spazio`), con `dispose()` completo alla chiusura. |
| **IV. Code-Driven Automatic Bilingual Taxonomy (No Rule-Builder UI)** | PASS | PASS | M2 non introduce pannelli UI di configurazione regole né tagging manuale; le euristiche di filtraggio banner/loghi e di significatività geometrica (`< 2.000` triangoli, `triangoli × diagonale`) sono deterministiche e versionate nel codice Rust. Lo schema `tags` / `model_tags` / `fingerprint_overrides` di M3 resta intatto. |
| **V. Zero Proprietary Licenses & Cross-OS WSL2 <-> Windows Verification** | PASS | PASS | Esclusivamente librerie open-source royalty-free (`image`, `quick-xml`, `tobj`, `gltf`, `wgpu`, `three`, `@react-three/fiber` v9, `@react-three/drei` v10); supporto `.lys`/`.lyt` limitato al Tier 1 clean-room senza SDK proprietari Mango3D. Il trait `MeshThumbnailRenderer` permette di testare tutta la pipeline Passata 2/3 in WSL2 senza hardware grafico, lasciando a Windows MSVC la verifica nativa `wgpu`/DX12/WebView2. |

**Verifica Migrazione & Continuità M1:** La migrazione [004_m2_thumbnails_and_geometry.sql](./data-model.md#2-migrazione-additiva-004_m2_thumbnails_and_geometrysql) aggiunge solo `preview_state`, `geometry_state`, `pass3_error` su `files` e la tabella `file_references`, preservando al 100% `sources`, `folders`, `models`, `files`, `thumbnails`, `models_fts`, `files_fts` e gli ID M1 esistenti. Nessuna violazione della Costituzione presente: *Complexity Tracking* resta vuoto.

---

## Project Structure

### Documentation (this feature)

```text
specs/002-thumbnails-3d-viewer/
├── spec.md                    # Specifica funzionale M2 e chiarificazioni (input)
├── plan.md                    # Questo file (/speckit-plan)
├── research.md                # Scelte tecniche, parser, wgpu trait, assetProtocol, R3F v9
├── data-model.md              # Migrazione 004, stati Passata 2/3, deduplicazione cache, file_references
├── quickstart.md              # Scenari di verifica automatici WSL2 + E2E Windows 10/11
└── contracts/
    └── tauri-ipc.md           # Estensioni additive DTO, assetProtocol, binary MSH1, comandi Pause/Resume/Viewer
```

### Source Code (repository root)

```text
package.json                   # Aggiunta three, @react-three/fiber@^9, @react-three/drei@^10, @types/three
src/
├── App.tsx                    # Stato esclusivo Inspector (reset 2D su cambio card) <-> QuickLook (Spazio/Esc), thumbsRoot, Pause/Resume
├── api/
│   ├── tauriClient.ts         # Estensioni IPC: get_thumbs_root, pause/resume_index_scan, load_mesh_bytes_for_viewer, load_image_for_viewer
│   └── useCatalogRead.ts      # Mapping nuovi codici IpcErrorCode (CLOUD_PLACEHOLDER_BLOCKED, CONFIRMATION_REQUIRED, ecc.)
├── components/
│   ├── TopCommandBar.tsx      # Supporto Passata 1/2/3 e controlli Pausa / Riprendi / Annulla
│   ├── CatalogGrid.tsx        # Altezza riga item adattata per card con miniature, onInspect(modelId | null, itemId) e virtualizzazione fissa
│   ├── ModelOrFileCard.tsx    # Miniatura .webp da assetProtocol + Hover Scrub CSS 4-angoli + click/<article> compatibile per immagini modelId=null
│   ├── InspectorPanel.tsx     # Evoluzione di TextInspectorPanel: 2D/metadati default, lytReferences, metriche mm/triangoli, switch 3D on-demand + selettore pezzi pack/ZIP
│   ├── QuickLookModal.tsx     # Modale rapido (Spazio/Esc): 3D on-demand o anteprima 2D immagini, blocco sicuro Online-Only, conferma >100MB / >3M tri
│   └── ThreeViewport.tsx      # Lazy-loaded (@react-three/fiber): parser binario MSH1, clay/matcap studio, OrbitControls + reset, wireframe/griglia/assi, dispose() completo
├── styles/
│   └── theme.css              # Conserva token M1 (#121417/#1B1F24/#272C33/#C5DBAF) + stili thumb, scrub dots e QuickLook
└── types/
    └── catalog.ts             # Estensioni DTO allineate a contracts/tauri-ipc.md

src-tauri/
├── Cargo.toml                 # Eventuali feature/dipendenze condivise workspace
├── tauri.conf.json            # security.assetProtocol enable + scope ["$APPLOCALDATA/cache/thumbs/**"] + CSP img-src
└── crates/
    └── meshcatalog-core/
        ├── Cargo.toml         # Aggiunta image, quick-xml, tobj, gltf, wgpu (con feature minime)
        ├── migrations/
        │   └── 004_m2_thumbnails_and_geometry.sql
        └── src/
            ├── contracts.rs   # ThumbnailDto, GeometryMetricsDto, ScanStatusDto (isPaused, pass 3), estensioni DTO M1
            ├── error.rs       # Nuovi IpcError/IpcErrorCode M2 + details opzionale per CONFIRMATION_REQUIRED
            ├── db/
            │   ├── migrations.rs # Registrazione 004_m2_thumbnails_and_geometry.sql
            │   ├── queries.rs    # Join thumbnails/metriche in search_catalog & get_model_detail (+ file_references .lyt)
            │   └── writer.rs     # Upsert thumbnails deduplicati, aggiornamento hero_thumb_id, file_references, stati Pass 2/3, prune orfani cache
            ├── thumbs/
            │   ├── mod.rs        # Cache manager (%LOCALAPPDATA%/cache/thumbs/<ab>/<hash>.webp), filtering banner/logo, encoding WebP
            │   ├── extractors.rs # Estrattori Pass 2: creator_image (disco/ZIP), 3mf Metadata/plate_*.png, lys preview.png, lyt refs
            │   └── renderer.rs   # Trait MeshThumbnailRenderer + WgpuThumbnailRenderer headless + MockThumbnailRenderer
            ├── mesh/
            │   ├── mod.rs        # NormalizedMesh, calcolo bbox mm/triangoli, hero score, serializzatore binario MSH1
            │   ├── stl.rs        # Parser STL binario & ASCII con pre-check budget
            │   ├── obj.rs        # Parser OBJ (tobj/BufRead + fan triangulation)
            │   ├── ply.rs        # Parser PLY ASCII / binary LE & BE
            │   ├── glb.rs        # Parser GLB (gltf + gerarchia nodi + m -> mm)
            │   └── threemf.rs    # Parser 3MF (ZIP + quick-xml + unit -> mm + component/build transforms)
            └── pipeline/
                ├── mod.rs        # Estensione Pass 2 (preview + lyt) & Pass 3 (Fase 3A + Fase 3B) + run_pending_passes + PauseGate
                └── ...
├── src/
│   ├── lib.rs                 # Setup cache/thumbs dir, startup auto-resume pending Pass 2/3, registrazione comandi M2
│   ├── commands.rs            # get_thumbs_root, pause_index_scan, resume_index_scan, load_mesh_bytes_for_viewer, load_image_for_viewer
│   └── scanner_service.rs     # Integrazione PauseGate, auto-resume all'avvio e throttling batch Pass 2/3
└── tests/ (meshcatalog-core/tests/ + tests/*.test.mjs)
    ├── m2_preview_extractors_tests.rs
    ├── m2_mesh_parsers_and_hero_tests.rs
    ├── m2_pass3_queue_resume_pause_tests.rs
    └── viewer_and_scrub.test.mjs
```

**Structure Decision**: Mantenuta invariata la separazione M1 **Cargo Workspace (`meshcatalog-core` + `meshcatalog-app`) + Frontend Vite React/TypeScript (`src/`)**. Tutti i parser mesh, gli estrattori `.3mf`/`.lys`/`.lyt`, la cache `.webp`, la coda Passata 3 a due fasi e l'astrazione `MeshThumbnailRenderer` vivono in `meshcatalog-core` così da garantire verifica automatica completa in WSL2; `src-tauri/src/` ospita il solo adattatore IPC Tauri (`tauri::ipc::Response`, `assetProtocol` scope, `ScannerService` auto-resume e pause gate).

---

## Complexity Tracking

*Nessuna violazione della Costituzione presente: tabella non applicabile.*
