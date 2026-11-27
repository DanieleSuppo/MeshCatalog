# Research & Architectural Decisions: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Branch**: `002-thumbnails-3d-viewer` | **Date**: 2026-11-27  
**Spec**: [spec.md](./spec.md) | **Master Architecture**: [docs/02-ARCHITECTURE.md](../../docs/02-ARCHITECTURE.md), [docs/04-UI-UX-AND-IPC.md](../../docs/04-UI-UX-AND-IPC.md)

Questo documento consolida tutte le scelte tecnologiche, librerie open-source, soglie operative e strategie cross-OS per la Milestone M2, eliminando ogni ambiguità prima della fase di progettazione dati e contratti.

---

## 1. Estrazione Immagini Creator, `.3mf`, `.lys`/`.lyt` & Codifica `.webp` (`meshcatalog-core`)

### Decisione
- **Decodifica immagini & ridimensionamento (`image`):** Adottare il crate `image` (MIT/Apache-2.0, `default-features = false`, feature `["jpeg", "png", "webp"]`) per decodificare `.jpg`, `.jpeg`, `.png`, `.webp` (sia su disco sia in buffer RAM da `.zip`/`.3mf`/`.lys`), filtrare asset non pertinenti ed eseguire il downscale `256×256` (`Triangle`/`Lanczos3`) e la codifica `.webp` (`image-webp` pure-Rust integrata in `image`, oppure `webp` / `libwebp` BSD-3-Clause per compressione lossy opzionale).
- **Parsing `.3mf` (`zip` + `quick-xml`):** Riutilizzare `zip` (già presente in `meshcatalog-core`) per leggere `Metadata/plate_*.png` e i file XML interni (`3D/3dmodel.model` e sub-modelli `3D/Objects/*.model` tipici di Bambu Studio / OrcaSlicer) con `quick-xml` (MIT/Apache-2.0, pure-Rust streaming XML).
- **Supporto Lychee Tier 1 (`.lys` / `.lyt`) senza SDK proprietari:**
  - `.lys`: ispezione clean-room in memoria con tetto di lettura sicuro (`<= 32 MB`): (1) tentativo lettura contenitore ZIP (`preview.png` / entry terminante in `preview.png`), (2) in alternativa estrazione JSON/base64 (`preview`) o scansione chunk PNG delimitato (`\x89PNG\r\n\x1a\n` ... `IEND`) validato tramite `image::load_from_memory_with_format(..., ImageFormat::Png)`.
  - `.lyt`: ispezione clean-room di testo/JSON/ZIP per estrarre in ordine i riferimenti a file `.stl` (e `.obj`/`.3mf`) collegati senza mai aprire né idratare i percorsi esterni referenziati.

### Rationale
- Zero dipendenze C/C++ obbligatorie per parsing immagini e XML, garantendo build rapide e identiche in WSL2 Linux e Windows MSVC (Principio V).
- La validazione PNG/JPEG/WebP dopo estrazione evita che byte casuali dentro un `.lys` vengano salvati come miniature corrotte.
- Il filtraggio euristicamente ordinato delle immagini creator (esclusione/deprioritizzazione di `logo`, `banner`, `icon`, `watermark`, `avatar`, `qr`, `patreon`, `socials`, `badge`, dimensioni `< 48 px`, aspect ratio estremi `> 4.5` o `< 0.22`) preserva il fallback pulito quando nessuna immagine migliore è disponibile.

### Alternatives Considered
- **Libreria proprietaria Mango3D / Assimp C++:** Scartata per violazione diretta del Principio V (licenze proprietarie e complessità build nativa).
- **Decodifica immagini lato WebView2/JS:** Scartata perché violerebbe il vincolo read-only/anti-idratazione sugli archivi `.zip` e impedirebbe il caching unificato `.webp` su disco in `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\`.

---

## 2. Parsing Open-Source dei Formati 3D & Metriche Geometriche (`meshcatalog-core`)

### Decisione
Implementare in `meshcatalog-core::mesh` un layer unificato di parsing in streaming / buffer limitato (`NormalizedMesh`: `vertices: Vec<[f32; 3]>`, `normals: Vec<[f32; 3]>`, `triangle_count: u32`, `bbox_mm: [f32; 3]`) con unità normalizzate in **millimetri (`mm`)**:
- **`.stl` (binario e ASCII):** Parser pure-Rust dedicato con controllo preventivo del conteggio triangoli nell'header binario (rifiuto immediato prima di allocare se `triangle_count > soglia_sicura` o se `50 * triangle_count + 84 != len` troncato) e supporto ASCII `solid ... facet normal ... vertex`. Unità assunte in `mm`.
- **`.obj`:** `tobj` (MIT, pure-Rust, `BufRead` da memoria/file) oppure parser `BufRead` dedicato con triangolazione a ventaglio per poligoni `N`-gon e indici negativi relativi. Unità assunte in `mm`.
- **`.ply`:** Parser pure-Rust dedicato per header ASCII e payload `ascii`, `binary_little_endian`, `binary_big_endian` con triangolazione a ventaglio delle facce e tetto esplicito su `element vertex` / `element face` per evitare OOM su header corrotti. Unità assunte in `mm`.
- **`.glb`:** Crate `gltf` (MIT/Apache-2.0, `default-features = false`, parsing binario GLB in RAM) con applicazione delle trasformazioni gerarchiche dei nodi (`matrix` / `TRS`) e conversione **metri $\rightarrow$ millimetri (`× 1000.0`)** secondo specifica glTF 2.0.
- **`.3mf`:** Lettura da `zip` + `quick-xml` di `<model unit="...">` (`millimeter ×1`, `centimeter ×10`, `meter ×1000`, `inch ×25.4`, `foot ×304.8`, `micron ×0.001`), componenti multi-oggetto (`3D/Objects/*.model`) e matrici affini `3×4` di `<component>` e `<build><item>`.
- **Selezione Pezzo Principale nei Pack (`FR-006`):**
  - Punteggio di significatività geometrica: `score = (triangle_count as f64) * bbox_diagonal_mm`.
  - Esclusione automatica dei micro-componenti con `triangle_count < 2_000` (perni, spine, basette lisce) ogniqualvolta nello stesso pack esista almeno una mesh valida con `triangle_count >= 2_000`.

### Rationale
- Un unico `NormalizedMesh` in `meshcatalog-core` garantisce coerenza al 100% tra statistiche SQLite (`triangle_count`, `bbox_x/y/z_mm`), miniatura `wgpu` a 4 angolazioni e mesh mostrata nel viewer Three.js.
- Controllare il budget prima dell'allocazione (`<= 100 MB` in Passata 3 automatica; `<= 100 MB` diretto / `100–500 MB` o `> 3M` triangoli con conferma / `> 500 MB` bloccato nel Viewer) previene picchi di memoria su statue multi-milione o file malformati.

### Alternatives Considered
- **Usare `three-stdlib` (`STLLoader`, `PLYLoader`, `3MFLoader`) nel frontend:** Scartato perché richiederebbe doppio parsing (Rust per Passata 3 + JS per Viewer), duplicherebbe la logica di unità/trasformazioni `.3mf` e trasferirebbe file grezzi fino a `500 MB` dentro WebView2 prima di sapere se hanno `> 3M` triangoli.

---

## 3. Rendering Headless Multi-Angolo (`wgpu`) & Testabilità in WSL2

### Decisione
- Introdurre in `meshcatalog-core` il trait **`MeshThumbnailRenderer: Send + Sync`** (`render_four_angles(&self, mesh: &NormalizedMesh) -> Result<Vec<u8>, RenderError>`) che restituisce un frame buffer RGBA `1024×256` (4 viste `256×256` affiancate orizzontalmente: `0°`, `90°`, `180°`, `270°` con pitch ~`25°` e auto-framing sul centroide/raggio del bounding box).
- **Implementazione di produzione (`WgpuThumbnailRenderer`):** Usa `wgpu` (MIT/Apache-2.0) in modalità **headless off-screen** (nessuna `Surface` o finestra):
  - Ordine di ricerca adattatore: `HighPerformance` $\rightarrow$ `LowPower` $\rightarrow$ `force_fallback_adapter: true` (software/WARP su Windows).
  - Pipeline render 3D con depth buffer (`Depth32Float`), backface/frontface normal shading a studio neutro (colore argilla opaca `#B8BDC4` con luce chiave, luce di riempimento e rim light morbida su sfondo scuro `#1B1F24`), supersampling `512×512` per vista con downsample a `256×256` per bordi anti-aliased nitidi.
  - Se nessun adattatore hardware o software è disponibile sulla macchina host, restituisce `RenderError::NoAdapter` permettendo al worker Passata 3 di salvare comunque `triangle_count` e `bbox_x/y/z_mm`, marcare il file in stato terminale `skipped_no_adapter` e notificare un avviso recuperabile senza bloccare il resto dell'app.
- **Test Double per WSL2 (`MockThumbnailRenderer`):** Nei test unitari e di integrazione di `meshcatalog-core` in WSL2 Linux (privo di display/GPU), un renderer deterministico pure-Rust produce strisce `1024×256` valide per verificare end-to-end cache, deduplicazione, batch updates, priorità Passata 3 e ripresa automatica.

### Rationale
- Rispecchia esattamente il pattern `EntryMetadataInspector` di M1 (`OsMetadataInspector` + `MockMetadataInspector`), mantenendo `cargo test -p meshcatalog-core` veloce e deterministico in WSL2 pur compilando e verificando il codice `wgpu` reale su Windows MSVC.

### Alternatives Considered
- **Software rasterizer pure-Rust come unico renderer:** Scartato per tempi di render su mesh da 1–3M triangoli (secondi per mesh su CPU vs millisecondi su `wgpu` DX12/Vulkan); il software rasterizer o il fallback WARP resta utile solo come ripiego su host privi di GPU dedicata.

---

## 4. Scoped Thumbnail Serving (`assetProtocol`) & Zero-Latency Hover Scrub

### Decisione
- **Protocollo Asset Tauri v2 con scope minimo:** Abilitare in `src-tauri/tauri.conf.json` `app.security.assetProtocol` con `enable: true` e `scope: ["$APPLOCALDATA/cache/thumbs/**"]` (aggiornando la CSP `img-src` per consentire `asset: http://asset.localhost`). Le cartelle utente e Dropbox **non** vengono mai aggiunte allo scope del protocollo asset.
- **Risoluzione URL O(1) senza IPC per card:** All'avvio, il frontend invoca una sola volta `get_thumbs_root()` per ottenere il percorso canonico di `%LOCALAPPDATA%\MeshCatalog\cache\thumbs`; ogni card compone l'URL locale tramite `convertFileSrc` combinando `thumbsRoot` e `thumbnail.relWebpPath` (`<ab>/<hash>.webp`).
- **Hover Scrub a 4 angolazioni a zero re-render React:** Quando `thumbnail.frameCount === 4`, `ModelOrFileCard` usa un contenitore CSS con `background-image`, `background-size: 400% 100%` e `background-position-x: calc(var(--scrub-frame, 0) * 33.3333%)`. Il listener `onPointerMove` aggiorna direttamente la CSS custom property `--scrub-frame` (`0..3`) sul nodo DOM senza `setState` né re-render React, garantendo `< 16 ms` durante lo scorrimento e lo scrub.

### Rationale
- Garantisce zero traffico IPC durante lo scroll e zero contesti WebGL nella griglia (Principio III).
- Impedisce strutturalmente che la WebView possa mai accedere via `asset://` a file fuori dalla cache miniature locale (Principi I e II).

### Alternatives Considered
- **Custom URI scheme `thumb://` in Rust:** Valida alternativa, ma `assetProtocol` nativo di Tauri v2 è già gestito nativamente da `convertFileSrc` su Windows WebView2 e rispetta lo scope dichiarativo `$APPLOCALDATA/cache/thumbs/**` senza implementare range/MIME/caching HTTP manuali.

---

## 5. Trasferimento Mesh al Viewer 3D (`tauri::ipc::Response` binario) & Stack Three.js React 19

### Decisione
- **Versioni Frontend compatibili con React 19:** Usare **`three`** (`^0.170`), **`@react-three/fiber`** (`^9.x`), **`@react-three/drei`** (`^10.x`) e **`@types/three`**, poiché `@react-three/fiber` v8 non supporta React 19 (`package.json` usa `react: ^19.0.0`).
- **Caricamento Lazy (`React.lazy`):** Il componente `ThreeViewport` viene caricato tramite `React.lazy` solo quando l'utente attiva il 3D nell'Inspector o apre il QuickLook su una mesh 3D, mantenendo il bundle iniziale della griglia leggero e privo di WebGL attivo.
- **Contratto IPC Binario (`tauri::ipc::Response`):** Il comando `load_mesh_bytes_for_viewer(fileId, allowRemoteFetch, confirmLarge)` esegue in Rust:
  1. Controllo `storage_state` (`cloud_placeholder` / `dropbox_remote` bloccati con `IpcError::CloudPlaceholderBlocked`, 0 byte scaricati).
  2. Verifica `attr_guard::open_resident_checked` sul file (o sull'archivio `.zip` padre estraendo in RAM la sola entry richiesta).
  3. Controllo soglie: `> 500 MB` $\rightarrow$ `MeshTooLargeBlocked`; `100–500 MB` senza `confirmLarge` $\rightarrow$ `ConfirmationRequired`; parsing in `NormalizedMesh` + controllo `triangle_count > 3_000_000` senza `confirmLarge` $\rightarrow$ `ConfirmationRequired`.
  4. Serializzazione compatta in `Vec<u8>` con header `MSH1` (`28 byte`: magic, version, triangleCount, bboxX/Y/Z, vertexCount) + array contigui `Float32Array` `positions` e `normals`, restituita come `tauri::ipc::Response` (`ArrayBuffer` diretto in JS senza serializzazione JSON `[u8]`).
- **Singola Istanze 3D & Rilascio Risorse (`FR-012`, `FR-015`):**
  - `App.tsx` coordina lo stato esclusivo tra Inspector 3D (disattivato di default su selezione e smontato al cambio scheda) e QuickLook (`Spazio` fuori da input testuali / `Esc` per chiudere). Quando QuickLook è attivo, l'eventuale viewport 3D dell'Inspector è smontata.
  - Al `useEffect` cleanup di `ThreeViewport`, chiamare esplicitamente `geometry.dispose()`, `material.dispose()` e rilasciare i riferimenti al `BufferGeometry` per azzerare i buffer VRAM/RAM entro `< 1 s`.

### Rationale
- `tauri::ipc::Response` evita l'overhead 5–8× di memoria e CPU che la serializzazione JSON di `Vec<u8>` causerebbe su mesh da 100–500 MB.
- Inviare geometria già normalizzata in `mm` da Rust elimina la duplicazione di 5 parser JS e assicura che i modelli `.3mf` multi-componente o `.glb` in metri abbiano scala e bounding box identici tra anteprima e viewer interattivo.

### Alternatives Considered
- **Restituire `Vec<u8>` JSON o path file su disco temporaneo:** JSON esplode in RAM oltre ~30 MB; scrivere file temporanei da `.zip` su disco violerebbe la pulizia read-only/no-extraction e aumenterebbe l'I/O su SSD.

---

## 6. Gestione Stato Terminale Pass 3, Auto-Resume all'Avvio & Pausa/Riprendi

### Decisione
- Aggiungere in migrazione `004_m2_thumbnails_and_geometry.sql` le colonne additive su `files`:
  - `preview_state TEXT NOT NULL DEFAULT 'pending'` (`'pending' | 'extracted' | 'not_applicable' | 'skipped' | 'failed'`)
  - `geometry_state TEXT NOT NULL DEFAULT 'pending'` (`'pending' | 'done' | 'skipped_size' | 'skipped_triangles' | 'skipped_no_adapter' | 'failed' | 'not_applicable'`)
  - `pass3_error TEXT`
- Nella migrazione `004`, le righe `cloud_placeholder` esistenti vengono aggiornate a `preview_state = 'not_applicable'`, `geometry_state = 'not_applicable'` (e in `write_inventory_batch` ogni `cloud_placeholder` riceve gli stessi valori terminali, mentre un file che cambia stato/dimensione/`mtime` torna a `'pending'`).
- All'avvio dell'app, `ScannerService::resume_pending_at_startup` verifica se esistono righe `files` con `storage_state IN ('local_resident', 'zip_entry')` e `(preview_state = 'pending' OR geometry_state = 'pending')`; in caso affermativo accoda una coda **solo Pass 2B (preview) + Pass 3 (geometria/render)** senza ripetere Pass 1.
- I worker Pass 2/3 interrogano un `PauseGate` (`AtomicBool` + `Mutex/Condvar`) prima di ogni unità di lavoro e alla fine di ogni batch, implementando `pause_index_scan`, `resume_index_scan` e `cancel_index_scan` senza perdere il lavoro già committato.

### Rationale
- Garantisce che i file saltati (`> 100 MB`, `> 3M` triangoli, mesh corrotte, `.gcode`, `.step`, `.stp`, o host privo di GPU) passino a uno stato terminale persistito in SQLite e **non vengano mai riaccodati** ai riavvii successivi.
- Assicura transizione fluida dai database M1 esistenti senza richiedere all'utente di ri-scansionare manualmente i 40 GB locali.
