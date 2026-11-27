# Quickstart & Validation Guide: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Branch**: `002-thumbnails-3d-viewer` | **Date**: 2026-11-27  
**Plan**: [plan.md](./plan.md) | **Contracts**: [contracts/tauri-ipc.md](./contracts/tauri-ipc.md)

Questa guida definisce i prerequisiti, i comandi di verifica automatica in **WSL2 (Linux)** e gli scenari di validazione end-to-end su **Windows 10/11 (`x86_64-pc-windows-msvc`)** per la Milestone M2.

---

## 1. Prerequisiti

### Su WSL2 (Pure-Rust Core, Parser/Extractor Tests & Frontend Build)
- **Rust stable** (`cargo`, `rustc`, `rustfmt`, `clippy`) con `src-tauri/Cargo.lock` aggiornato.
- **Node.js 22 LTS** (`>= 22.12.0`) + `npm` con `package-lock.json` aggiornato (`three`, `@react-three/fiber` v9, `@react-three/drei` v10).
- Nessun server grafico X11/Wayland o GPU hardware richiesto per i test di `meshcatalog-core` (i test di pipeline Passata 2/3 usano il renderer deterministico `MockThumbnailRenderer`).

### Su Windows 10/11 (Headless `wgpu`, Asset Protocol, Three.js WebView2 & Smart Sync)
- **Rust toolchain (`x86_64-pc-windows-msvc`)** + Visual Studio C++ Build Tools.
- **Node.js LTS** + **Edge WebView2 Runtime** con supporto WebGL2.
- **Tauri CLI v2** (`npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked`).

---

## 2. Validazione in WSL2 (Pure-Rust Core + Frontend TypeScript/Vite)

1. **Verifica formattazione, lint e test di `meshcatalog-core`:**
   ```bash
   cargo fmt --check --manifest-path src-tauri/Cargo.toml
   cargo clippy --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --all-targets -- -D warnings
   cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core
   ```
   **Esiti attesi:**
   - **Migrazione `004` & Upgrade M1 $\rightarrow$ M2:** Apertura di un database M1 v3 aggiorna a v4 preservando modelli/file/FTS5, marcando i `local_resident`/`zip_entry` esistenti come pendenti per Passata 2/3 e i `cloud_placeholder` come `not_applicable`; la riesecuzione dell'auto-resume non riaccoda i file in stato terminale (`skipped_size`, `skipped_triangles`, `failed`, `not_applicable`).
   - **Estrattore Passata 2 (`FR-001`..`FR-003`):**
     - Immagini creator valide (`.jpg`/`.png`/`.webp`, anche da `.zip` e non associate `model_id = NULL`) convertite in `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\<ab>\<hash>.webp`; banner/loghi e icone `< 48 px` scartati quando esistono render validi.
     - `.3mf` con `Metadata/plate_1.png` estratto in memoria senza scrivere su disco utente.
     - `.lys` con `preview.png` estratta in Tier 1; `.lyt` con riferimenti `.stl`/`.obj`/`.3mf` salvati in `file_references` senza aprire percorsi esterni.
   - **Parser 3D, Metriche & Selezione Hero (`FR-005`, `FR-006`):**
     - `.stl` (binario e ASCII), `.obj`, `.ply`, `.glb` (`m -> mm`) e `.3mf` (`unit` -> `mm`) producono `triangle_count` e `bbox_x/y/z_mm` accurati; su corpus eterogeneo valido, copertura anteprime `>= 95%` (`SC-003`).
     - Selezione Hero nei pack multi-file preferisce sempre `creator_image` > `3mf_plate` > `lys_preview` > mesh 3D con punteggio `triangoli × diagonale`, ignorando micro-componenti `< 2.000` triangoli in presenza di pezzi più ricchi.
     - Mesh `> 100 MB` o `> 3M` triangoli o corrotte vengono marcate in stato terminale con avviso recuperabile senza bloccare la coda.
   - **Coda Due Fasi, Cache Deduplicata & Pausa/Riprendi (`FR-006`, `FR-007`, `FR-009`):**
     - Priority 1 (copertine pack scoperti + `single_file`) completata prima di Priority 2 (altre mesh interne dei pack).
     - Spostamento/rinomina di cartella pack o `.zip` riutilizza la miniatura esistente per `cache_key` senza rigenerazione.
     - `attr_guard` blocca ogni lettura su `cloud_placeholder` (**0 aperture dati** in Passata 2, Passata 3 e `load_mesh_bytes_for_viewer`).

2. **Verifica Frontend TypeScript, Bundle & Test UI/Viewer:**
   ```bash
   npm ci
   npm run build
   npm test
   ```
   **Esito atteso:**
   - Zero errori `tsc --noEmit`; Three.js (`ThreeViewport`) isolato in chunk lazy separato nel build Vite.
   - Test automatizzati su `ModelOrFileCard` (hover scrub 4 viste, fallback pulito, selezionabilità immagini `modelId === null` senza chiamare `get_model_detail`), `TopCommandBar` (`Pausa` / `Riprendi` / `Annulla` su Passata 1–3), gating `QuickLook` con `Spazio` fuori dai campi di input e coordinamento singola istanza 3D tra Inspector e QuickLook.

---

## 3. Validazione End-to-End su Windows 10/11 (`x86_64-pc-windows-msvc`)

Dopo `git pull` su Windows, compilare ed eseguire i test del workspace completo inclusi `wgpu` e `meshcatalog-app`:

```powershell
npm ci
npm run build
cargo check --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --workspace
cargo test --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --workspace
npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked
```

### Scenario A — Zero-Hydration & Integrità Read-Only in Passata 2 e 3 (`SC-001`, `SC-002`)
1. Aprire una sorgente locale mista contenente file/ZIP residenti e segnaposto Dropbox *Online-Only* (immagini, `.3mf`, `.lys`, `.lyt`, `.stl`, `.zip`).
2. Monitorare il traffico I/O / Dropbox durante Passata 2 e Passata 3 e tentare di aprire un segnaposto *Online-Only* nel QuickLook (`Spazio`).
3. **Esito atteso:** **0 byte** scaricati dal cloud; nessun file creato o modificato nelle cartelle utente; le miniature `.webp` risiedono solo in `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\`; QuickLook sul segnaposto mostra messaggio esplicito *Online-Only* senza attivare download né WebGL.

### Scenario B — Estrazione Anteprime, Render Headless `wgpu` & Auto-Resume (`SC-003`, `SC-006`)
1. Avviare la scansione su un dataset misto (pack con foto creator + `logo.png`, `.3mf` con piastra, `.lys` con preview, `.lyt`, pack di sole mesh con micro-perni `< 2.000` triangoli + statua, `.zip` di sole mesh, file oltre `100 MB`).
2. Mettere in pausa (`Pausa`), chiudere l'app a metà Passata 3, riavviare l'app e verificare la ripresa automatica.
3. Spostare un pack e un `.zip` in Esplora File e rieseguire la scansione.
4. **Esito atteso:**
   - Al riavvio Passata 3 riprende automaticamente dagli elementi pendenti senza rieseguire Passata 1 e senza riaccodare i file già saltati (`> 100 MB` / `.gcode` / `.step`).
   - Le copertine rispettano la priorità `creator_image` > `3mf_plate` > `lys_preview` > mesh principale `wgpu`; gli spostamenti riusano istantaneamente le `.webp` in cache.

### Scenario C — Griglia `< 16 ms`, *Hover Scrub* 4-Viste & Viewer 3D On-Demand (`SC-004`, `SC-005`)
1. Popolare/aprire una vista con `10.000+` elementi dotati di miniature in cache; scorrere velocemente e passare il cursore orizzontalmente sulle schede generate da `wgpu`.
2. Selezionare un pack multi-file (o `.zip`) nell'Inspector: verificare apertura iniziale in **2D/metadati**, attivare **"Carica vista 3D"**, verificare cambio pezzo dentro lo `.zip` e ritorno a **2D** selezionando un'altra scheda.
3. Premere `Spazio` sulla griglia per aprire **QuickLook**: verificare orbita/pan/zoom/reset, toggle **wireframe / griglia / assi**, lettura `mm` e triangoli, conferma esplicita su mesh tra `100–500 MB`, anteprima 2D su immagine singola (`modelId === null`), chiusura con `Spazio` o `Esc` e stabilità memoria su 20 aperture/chiusure consecutive.
4. Verificare che premendo `Spazio` dentro la barra di ricerca venga inserito normalmente uno spazio senza aprire il QuickLook.
