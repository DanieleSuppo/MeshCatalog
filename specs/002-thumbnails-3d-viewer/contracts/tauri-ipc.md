# Tauri IPC Contracts: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Branch**: `002-thumbnails-3d-viewer` | **Date**: 2026-11-27  
**M1 Baseline**: [specs/001-core-local-indexer/contracts/tauri-ipc.md](../001-core-local-indexer/contracts/tauri-ipc.md) | **Master Reference**: [docs/04-UI-UX-AND-IPC.md](../../docs/04-UI-UX-AND-IPC.md)

Questo documento estende in modo additivo e retrocompatibile i contratti IPC di M1 (`Commands` e `Events`) per esporre miniature `.webp` in cache locale, metriche geometriche, riferimenti `.lyt`, controllo Pausa/Riprendi/Stop di Passata 2–3 e caricamento binario sicuro on-demand per il Viewer 3D e QuickLook (`Spazio`).

---

## 1. Estensioni ai Tipi Condivisi (`TypeScript <-> Rust serde camelCase`)

### 1.1 Nuovi Tipi M2

```typescript
export type ThumbnailOrigin =
  | 'creator_image'
  | '3mf_plate'
  | 'lys_preview'
  | 'wgpu_render'
  | 'dropbox_thumb';

export interface ThumbnailDto {
  id: number;
  relWebpPath: string; // percorso relativo sotto thumbsRoot, es. "ab/0123...webp"
  origin: ThumbnailOrigin;
  frameCount: 1 | 4;   // 1 = anteprima statica, 4 = striscia orizzontale 1024x256 per hover scrub
  width: number;
  height: number;
}

export interface GeometryMetricsDto {
  triangleCount: number;
  bboxXMm: number;
  bboxYMm: number;
  bboxZMm: number;
}

export interface ViewerMeshData {
  triangleCount: number;
  bboxXMm: number;
  bboxYMm: number;
  bboxZMm: number;
  vertexCount: number;
  positions: Float32Array; // lunghezza = vertexCount * 3 (coordinate in mm centrate sul bounding box)
  normals: Float32Array;   // lunghezza = vertexCount * 3
}
```

### 1.2 Estensioni Additive ai DTO M1

Tutti i campi M1 rimangono invariati; M2 aggiunge solo campi opzionali/nullabili:

```typescript
export interface ModelCardDto {
  // ...tutti i campi M1 invariati...
  heroThumbnail: ThumbnailDto | null;
}

export interface FileCardDto {
  // ...tutti i campi M1 invariati...
  thumbnail: ThumbnailDto | null;
  geometry: GeometryMetricsDto | null;
  lytReferences?: string[]; // valorizzato solo in get_model_detail per file .lyt; omesso in search_catalog
}

export interface ModelDetailDto {
  // ...tutti i campi M1 invariati...
  heroThumbnail: ThumbnailDto | null;
}

export interface ScanStatusDto {
  isRunning: boolean;
  isPaused: boolean;          // estensione M2 (false se nessun worker in pausa)
  activeSourceId: number | null;
  pass: 1 | 2 | 3 | null;     // unificato con IndexerPass (Passata 1, 2 o 3)
  phaseLabel: string | null;  // es. "Passata 2/3 — Estrazione anteprime", "Passata 3/3 — Rendering 3D"
  processed: number;
  total: number;
  currentItem: string | null;
  recentErrors: IndexerErrorEvent[];
}
```

---

## 2. Protocollo Asset & Comandi Tauri (`#[tauri::command]`)

### 2.1 Asset Protocol (Cache Miniature `.webp`)

- In `src-tauri/tauri.conf.json`:
  - `app.security.assetProtocol.enable = true`
  - `app.security.assetProtocol.scope = ["$APPLOCALDATA/cache/thumbs/**"]`
  - `app.security.csp` include `img-src 'self' asset: http://asset.localhost data: blob:`
- **Invariante di Sicurezza:** Nessuna cartella utente o Dropbox entra nello scope di `assetProtocol`. Il frontend valida che `relWebpPath` non contenga `..` e risolve gli URL solo tramite `convertFileSrc` a partire dal percorso restituito da `get_thumbs_root()`.

### 2.2 Nuovi Comandi M2 (e Comandi M1 Estesi)

| Comando | Input | Output | Comportamento / Invarianti |
| :--- | :--- | :--- | :--- |
| `get_thumbs_root` | `—` | `string` | Restituisce il percorso canonico assoluto della directory `%LOCALAPPDATA%\MeshCatalog\cache\thumbs` (creata se assente). Chiamato una sola volta all'avvio UI. |
| `pause_index_scan` | `sourceId?: number` | `void` | Sospende in sicurezza il worker attivo / la coda Passata 1–3 associata (`PauseGate`) senza perdere il lavoro già salvato su SQLite; aggiorna `ScanStatusDto.isPaused = true`. |
| `resume_index_scan` | `sourceId?: number` | `void` | Riprende l'esecuzione del worker sospeso (`isPaused = false`). |
| `cancel_index_scan` | `sourceId?: number` | `void` | M1 esteso: sveglia immediatamente eventuali worker sospesi su `PauseGate`, annulla le code residue e termina al confine della transazione/unità corrente (lo stesso risveglio vale per `remove_source` prima dell'attesa su `released`). |
| `start_index_scan` | `sourceId?: number` | `void` | M1 esteso: esegue in background Passata 1 $\rightarrow$ Passata 2 (hash + ZIP + anteprime + `.lyt`) $\rightarrow$ Passata 3 (Fase 3A copertine/singoli + Fase 3B dettagli pack). |
| `load_mesh_bytes_for_viewer` | `fileId: number, allowRemoteFetch?: boolean, confirmLarge?: boolean` | `ArrayBuffer` (`tauri::ipc::Response`) | Carica in RAM una mesh 3D residente (`local_resident` o `zip_entry` tramite `attr_guard::open_resident_checked`, senza scrivere file su disco utente). In M2 `allowRemoteFetch` è ignorato/rifiutato per i file non residenti: blocca sempre `cloud_placeholder`/`dropbox_remote` con `CLOUD_PLACEHOLDER_BLOCKED` (`0` byte scaricati); blocca `> 500 MB` con `MESH_TOO_LARGE_BLOCKED`; richiede `confirmLarge: true` tramite `CONFIRMATION_REQUIRED` se `100–500 MB` o `> 3M` triangoli; restituisce payload binario `MSH1` per Three.js. |
| `load_image_for_viewer` | `fileId: number` | `ArrayBuffer` (`tauri::ipc::Response`) | Carica in RAM i byte di un'immagine residente (`local_resident` o `zip_entry`, max `50 MB`) per QuickLook 2D ad alta risoluzione (anche per immagini `model_id = NULL`). Blocca `cloud_placeholder` con `CLOUD_PLACEHOLDER_BLOCKED`. |

---

## 3. Formato Binario `MSH1` (`load_mesh_bytes_for_viewer`)

Restituito direttamente come `tauri::ipc::Response` (`ArrayBuffer` raw in JS, zero serializzazione JSON):

| Offset (byte) | Campo | Tipo | Descrizione |
| :---: | :--- | :--- | :--- |
| `0..4` | `magic` | `[u8; 4]` | Costante ASCII `"MSH1"` |
| `4..8` | `version` | `u32` LE | Versione formato (`1`) |
| `8..12` | `triangleCount` | `u32` LE | Conteggio esatto triangoli (`vertexCount / 3`) |
| `12..16` | `bboxXMm` | `f32` LE | Dimensione reale `X` in millimetri |
| `16..20` | `bboxYMm` | `f32` LE | Dimensione reale `Y` in millimetri |
| `20..24` | `bboxZMm` | `f32` LE | Dimensione reale `Z` in millimetri |
| `24..28` | `vertexCount` | `u32` LE | Numero vertici (`triangleCount * 3`) |
| `28..28+12*V` | `positions` | `[f32; V*3]` LE | Coordinate triangoli in `mm` negli assi nativi della mesh centrate esattamente sul centro geometrico del bounding box (`(0, 0, 0)`), così che `ThreeViewport` posizioni la griglia del piano di stampa sotto il modello a `y = -bboxYMm / 2` |
| `28+12*V..28+24*V` | `normals` | `[f32; V*3]` LE | Normali per vertice (calcolate da faccia se assenti nel formato sorgente) |

---

## 4. Nuovi Codici Errore Strutturati (`IpcErrorCode`)

Estensione retrocompatibile di `IpcErrorCode` di M1:

```typescript
export type IpcErrorCode =
  // M1 invariati
  | 'SOURCE_NOT_FOUND'
  | 'SOURCE_UNAVAILABLE'
  | 'SOURCE_OVERLAP'
  | 'INVALID_PATH'
  | 'SCAN_ALREADY_RUNNING'
  | 'MODEL_NOT_FOUND'
  | 'DATABASE_ERROR'
  | 'INTERNAL_ERROR'
  // Estensioni M2
  | 'FILE_NOT_FOUND'
  | 'CLOUD_PLACEHOLDER_BLOCKED'
  | 'UNSUPPORTED_VIEWER_FORMAT'
  | 'CONFIRMATION_REQUIRED'
  | 'MESH_TOO_LARGE_BLOCKED'
  | 'CORRUPT_MESH_OR_ASSET';
```

- **`CONFIRMATION_REQUIRED`:** Includere nel payload serializzato `IpcError` un campo opzionale `details?: string` (JSON o testo strutturato, es. `{"sizeBytes":142000000,"triangleCount":4200000,"reason":"high_triangle_count"}`) così la UI mostra il dialogo *"File pesante (X MB / Y triangoli) — Carica comunque"* prima di invocare nuovamente con `confirmLarge: true`.

---

## 5. Eventi Asincroni Backend $\rightarrow$ Frontend (`listen`)

Tutti i canali M1 (`indexer://progress`, `indexer://batch_updated`, `indexer://error`, `indexer://completed`) restano invariati nei nomi e nella compatibilità strutturale:
- `indexer://progress`: ora emette `pass: 1 | 2 | 3` e `isPaused?: boolean` (opzionale, per sincronia immediata su `pause_index_scan` / `resume_index_scan`).
- `indexer://batch_updated`: emesso a batch (max ~250–300 ms o ~25 modelli/miniatura completati in Passata 2/3) così la UI aggiorna progressivamente le miniature delle sole card visibili senza ricaricare l'intera griglia né resettare lo scroll.
- `indexer://error`: emette avvisi `recoverable: true` per mesh corrotte, file `> 100 MB` / `> 3M` triangoli saltati in Passata 3, oppure assenza di adattatore GPU (`wgpu`).
