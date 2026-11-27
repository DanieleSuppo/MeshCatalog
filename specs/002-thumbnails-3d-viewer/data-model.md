# Data Model: Pipeline Grafica, Miniature e Viewer 3D On-Demand (`002-thumbnails-3d-viewer`)

**Branch**: `002-thumbnails-3d-viewer` | **Date**: 2026-11-27  
**Spec**: [spec.md](./spec.md) | **M1 Baseline**: [specs/001-core-local-indexer/data-model.md](../001-core-local-indexer/data-model.md) | **Master DDL**: [docs/02-ARCHITECTURE.md](../../docs/02-ARCHITECTURE.md)

Il modello dati di M2 estende in modo strettamente **non distruttivo** lo schema SQLite di M1 (`user_version = 3` $\rightarrow$ `4` tramite `004_m2_thumbnails_and_geometry.sql`), popolando le tabelle e colonne già predisposte (`thumbnails`, `models.hero_thumb_id`, `files.thumb_id`, `files.triangle_count`, `files.bbox_x_mm`, `files.bbox_y_mm`, `files.bbox_z_mm`, `files.index_pass`) e aggiungendo esclusivamente lo stato terminale di Passata 2/3 su `files` e i riferimenti `.lyt` in `file_references`.

---

## 1. Relazioni tra Entità (ER Overview M2)

```text
sources (1) ──< (N) folders (self-ref parent_id)
   │                    │
   │                    └──< (N) models (0..1) ──< (N) files (self-ref parent_archive_id)
   │                                │                   │       │
   └────────────────────────────────┼───────────────────┘       ├──< (N) file_references (.lyt -> .stl)
                                    │                   │       │
                                    ▼                   ▼       │
                              thumbnails (hero)   thumbnails (per-file)
```

---

## 2. Migrazione Additiva `004_m2_thumbnails_and_geometry.sql`

```sql
ALTER TABLE files ADD COLUMN preview_state TEXT NOT NULL DEFAULT 'pending'
    CHECK (preview_state IN ('pending', 'extracted', 'not_applicable', 'skipped', 'failed'));

ALTER TABLE files ADD COLUMN geometry_state TEXT NOT NULL DEFAULT 'pending'
    CHECK (geometry_state IN ('pending', 'done', 'skipped_size', 'skipped_triangles', 'skipped_no_adapter', 'failed', 'not_applicable'));

ALTER TABLE files ADD COLUMN pass3_error TEXT;

-- I segnaposto Online-Only non devono mai entrare in coda Pass 2/3 all'avvio.
UPDATE files
SET preview_state = 'not_applicable',
    geometry_state = 'not_applicable',
    index_pass = 3
WHERE storage_state = 'cloud_placeholder';

CREATE INDEX idx_files_pass2_pending
    ON files(source_id, preview_state)
    WHERE storage_state IN ('local_resident', 'zip_entry') AND preview_state = 'pending';

CREATE INDEX idx_files_pass3_pending
    ON files(source_id, geometry_state)
    WHERE storage_state IN ('local_resident', 'zip_entry') AND geometry_state = 'pending';

CREATE TABLE file_references (
    id       INTEGER PRIMARY KEY,
    file_id  INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    ord      INTEGER NOT NULL,
    ref_path TEXT NOT NULL,
    ref_kind TEXT NOT NULL DEFAULT 'lyt_stl',
    UNIQUE(file_id, ord)
);

CREATE INDEX idx_file_references_file ON file_references(file_id);
```

**Compatibilità Upgrade M1 $\rightarrow$ M2:**
- Tutti i record `local_resident` e `zip_entry` già presenti in un database M1 mantengono `id`, `model_id`, `content_hash`, `uri`, `FTS5` intatti ed ereditano `preview_state = 'pending'` e `geometry_state = 'pending'`.
- Al primo avvio della build M2, `ScannerService` rileva tali righe pendenti e riprende automaticamente Passata 2 (estrazione anteprime) e Passata 3 (geometria + rendering) senza rieseguire Passata 1.

---

## 3. Entità e Campi Principali in M2

### 3.1 `thumbnails` (Tabella esistente, popolata in M2)

| Colonna | Tipo SQLite | Vincoli / Significato M2 |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Identificativo referenziato da `models.hero_thumb_id` e `files.thumb_id` |
| `cache_key` | `TEXT NOT NULL UNIQUE` | Chiave deterministica basata su impronta contenuto (es. `img:<blake3>`, `3mf_plate:<blake3>`, `lys_preview:<blake3>`, `wgpu_v1:<blake3>`) |
| `rel_webp_path` | `TEXT NOT NULL` | Percorso relativo sotto `%LOCALAPPDATA%\MeshCatalog\cache\thumbs\` nel formato `<ab>/<hash>.webp` |
| `origin` | `TEXT NOT NULL` | `'creator_image' \| '3mf_plate' \| 'lys_preview' \| 'wgpu_render' \| 'dropbox_thumb'` |
| `frame_count` | `INTEGER NOT NULL DEFAULT 1` | `1` per immagini/plate/preview statiche; `4` per striscia multi-angolo `wgpu` |
| `width` | `INTEGER NOT NULL` | `256` (singola) o `1024` (striscia 4×256) |
| `height` | `INTEGER NOT NULL` | `256` |
| `created_at` | `INTEGER NOT NULL` | Unix timestamp creazione in cache |

**Regole di Deduplicazione & Pulizia Cache (`FR-007`):**
- Prima di decodificare/renderizzare in `cache/thumbs/<ab>/<hash>.webp`, il worker verifica se `cache_key` esiste già in `thumbnails` e se il file `.webp` corrispondente è presente su disco; in caso positivo riutilizza direttamente `thumbnails.id` senza ricalcolo né scrittura duplicata.
- Su spostamenti/rinomine riconciliati o su duplicità di contenuti tra pack/archivi diversi, più righe `files`/`models` condividono lo stesso `thumbnails.id`.
- La rimozione di file orfani in `thumbnails` + disco avviene **esclusivamente** al completamento di una scansione completa senza annullamento né coperture parziali (`!cancelled && !coverage.incomplete`), eliminando solo i record non referenziati da alcuna delle tre colonne `files.thumb_id`, `models.hero_thumb_id` e `folders.hero_thumb_id`.

---

### 3.2 Estensioni `files` in M2 (`CatalogFile`)

Campi di M1 preservati invariati più campi attivati/aggiunti in M2:

| Colonna | Tipo SQLite | Significato / Regole M2 |
| :--- | :--- | :--- |
| `triangle_count` | `INTEGER` | Numero di triangoli calcolato in Passata 3 (o on-demand nel Viewer) per `.stl`/`.obj`/`.ply`/`.glb`/`.3mf`; `NULL` se non applicabile, saltato per dimensione `>100 MB` o fallito |
| `bbox_x_mm` | `REAL` | Larghezza asse `X` in `mm` |
| `bbox_y_mm` | `REAL` | Profondità asse `Y` in `mm` |
| `bbox_z_mm` | `REAL` | Altezza asse `Z` in `mm` |
| `thumb_id` | `INTEGER` | Riferimento logico a `thumbnails(id)` per anteprima individuale in **Singoli File** e lista pezzi Inspector |
| `index_pass` | `INTEGER NOT NULL DEFAULT 1` | `1` = inventario, `2` = hash/zip/preview, `3` = terminale Passata 3 (eseguito, saltato o non applicabile) |
| `preview_state` | `TEXT NOT NULL DEFAULT 'pending'` | `'pending' \| 'extracted' \| 'not_applicable' \| 'skipped' \| 'failed'` |
| `geometry_state` | `TEXT NOT NULL DEFAULT 'pending'` | `'pending' \| 'done' \| 'skipped_size' \| 'skipped_triangles' \| 'skipped_no_adapter' \| 'failed' \| 'not_applicable'` |
| `pass3_error` | `TEXT` | Messaggio diagnostico recuperabile quando `geometry_state` è `skipped_*` o `failed` |

---

### 3.3 `file_references` (Metadati Riferimento `.lyt` Tier 1)

| Colonna | Tipo SQLite | Significato |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Chiave primaria |
| `file_id` | `INTEGER NOT NULL` | `REFERENCES files(id) ON DELETE CASCADE` (file `.lyt` sorgente) |
| `ord` | `INTEGER NOT NULL` | Ordine deterministico di comparsa (`0..N-1`) |
| `ref_path` | `TEXT NOT NULL` | Percorso sorgente (`.stl`, `.obj` o `.3mf`) estratto dal contenitore `.lyt` (solo testo informativo, mai aperto automaticamente) |
| `ref_kind` | `TEXT NOT NULL DEFAULT 'lyt_stl'` | Tipo riferimento estratto (`'lyt_stl' \| 'lyt_obj' \| 'lyt_3mf'`) |

---

## 4. Macchine a Stati (Passata 2 & Passata 3)

### 4.1 Stati di `preview_state` (Passata 2 — Estrazione Anteprime)

```text
                [Pass 1 / file modificato]
                           │
                           ▼
  cloud_placeholder ──► 'not_applicable' (0 byte letti)
  non-preview ext   ──► 'not_applicable' (.stl/.obj/.ply/.glb/.zip/.gcode/.step/.stp)
  .jpg/.png/.webp   ──► 'extracted' (thumb_id = creator_image)
                    ├──► 'skipped'   (banner/logo/icona scartata o dimensioni < 48px)
                    └──► 'failed'    (immagine corrotta / non decodificabile)
  .3mf              ──► 'extracted' (se Metadata/plate_*.png presente -> 3mf_plate)
                    ├──► 'not_applicable' (nessun plate PNG; passa a Pass 3 wgpu)
                    └──► 'failed'    (archivio .3mf corrotto)
  .lys              ──► 'extracted' (se preview.png presente -> lys_preview)
                    ├──► 'not_applicable' (nessuna preview Tier 1)
                    └──► 'failed'    (file illeggibile)
  .lyt              ──► 'extracted' (riferimenti salvati in file_references)
                    └──► 'not_applicable' / 'failed'
```

### 4.2 Stati di `geometry_state` (Passata 3 — Analisi & Render Headless)

```text
  cloud_placeholder / non-mesh (.jpg,.png,.webp,.lys,.lyt,.zip,.gcode,.step,.stp)
       └──► 'not_applicable' (index_pass = 3)

  Mesh (.stl, .obj, .ply, .glb, .3mf):
       ├── size_bytes > 100 MB            ──► 'skipped_size'      (pass3_error, index_pass = 3)
       ├── corrupt / 0 tri / bbox nullo   ──► 'failed'            (pass3_error, index_pass = 3)
       ├── triangle_count > 3_000_000     ──► 'skipped_triangles' (pass3_error, index_pass = 3)
       └── valid (<= 100 MB, <= 3M tri)   ──► salva triangle_count + bbox_x/y/z_mm
               ├── no GPU/software adapter ──► 'skipped_no_adapter' (pass3_error, index_pass = 3)
               └── render 4-viste OK       ──► 'done' (thumbnails wgpu_render; files.thumb_id se assente; index_pass = 3)
```

**Invariante Anti-Loop Avvio & Coerenza Aggiornamenti:**
- L'auto-resume all'avvio seleziona esclusivamente `storage_state IN ('local_resident', 'zip_entry') AND (preview_state = 'pending' OR geometry_state = 'pending')`. Ogni esito di Passata 2/3 sposta lo stato fuori da `'pending'`, garantendo che file pesanti, corrotti o non grafici non rientrino mai in coda nei riavvii successivi a meno che i metadati di origine cambino.
- **Reset su file modificati (`write_inventory_batch` e `write_analysis`):** quando un file `local_resident` modifica `(storage_state, size_bytes, mtime_unix)` oppure una `zip_entry` modifica `(size_bytes, zip_entry_csize, zip_entry_offset, zip_entry_method)`, l'`ON CONFLICT(uri) DO UPDATE` resetta esplicitamente `preview_state = 'pending'` (o `'not_applicable'` se estensione non preview), `geometry_state = 'pending'` (o `'not_applicable'` se estensione non mesh), `pass3_error = NULL`, `thumb_id = NULL`, `triangle_count = NULL`, `bbox_x_mm = NULL`, `bbox_y_mm = NULL`, `bbox_z_mm = NULL`. I record `cloud_placeholder` forzano invece `preview_state = 'not_applicable', geometry_state = 'not_applicable', index_pass = 3`.
- **Conservazione `hero_thumb_id` negli spostamenti (`reconcile_move`):** quando uno spostamento 1:1 riconcilia il modello `original` con `destination`, `reconcile_move` copia `coalesce(destination.hero_thumb_id, models.hero_thumb_id)` sulla riga `models` preservata (`original`) evitando la perdita della copertina.

---

## 5. Regole di Priorità & Selezione Deterministica

1. **Priorità `models.hero_thumb_id` (`FR-004`):**
   1. `creator_image` (`files.thumb_id` con `origin = 'creator_image'`, preferendo file non scartati con maggior risoluzione pixel `width * height`, tie-break ordinamento alfabetico `rel_path`).
   2. `3mf_plate` (`files.thumb_id` con `origin = '3mf_plate'`, preferendo `plate_1.png` rispetto a varianti secondarie).
   3. `lys_preview` (`files.thumb_id` con `origin = 'lys_preview'`).
   4. `wgpu_render` del pezzo 3D con maggior punteggio di significatività geometrica:
      - Insieme candidati: tutte le mesh 3D valide del modello con `triangle_count >= 2_000`; se vuoto, tutte le mesh 3D valide del modello.
      - Punteggio: `triangle_count * sqrt(bbox_x_mm^2 + bbox_y_mm^2 + bbox_z_mm^2)`.
2. **Coda Passata 3 a Due Fasi (`FR-006`):**
   - **Priorità 1:** un pezzo candidato (stimato per dimensione file/estensione o punteggio se già noto, escludendo file `> 100 MB`) per ciascun `folder_pack`/`zip_archive` con `hero_thumb_id IS NULL` e ciascun `single_file` con `thumb_id IS NULL`. Se la stima iniziale fallisce (`> 3M` triangoli o mesh corrotta), il worker prova il candidato successivo dello stesso pack prima di chiudere il pack.
   - **Priorità 2:** tutte le restanti mesh `geometry_state = 'pending'` (inclusi gli altri pezzi di pack già coperti da foto/plate/lys).
