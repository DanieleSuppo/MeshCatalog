# Data Model: Core Local Indexer (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26  
**Spec**: [spec.md](./spec.md) | **Master DDL**: [docs/02-ARCHITECTURE.md](../../docs/02-ARCHITECTURE.md)

Il modello dati di M1 adotta lo schema SQLite DDL definito in `docs/02-ARCHITECTURE.md` (`WAL` mode, `foreign_keys = ON`, `FTS5`), arricchendolo con le colonne additive necessarie a supportare in M1 le anteprime ricorsive delle sottocartelle, lo stato operativo delle sorgenti e la ricerca `< 100 ms` su `Modelli / Pack` e `Singoli File`.

---

## 1. Relazioni tra Entità (ER Overview)

```text
sources (1) ──< (N) folders (self-ref parent_id)
   │                    │
   │                    └──< (N) models (1) ──< (N) files (self-ref parent_archive_id)
   │                                                  │
   └──────────────────────────────────────────────────┘
   
models (1) ── (1) models_fts [FTS5]
files  (1) ── (1) files_fts  [FTS5]

[Tabelle predisposte nello schema iniziale per M2/M3 senza rotture DDL]
thumbnails, tags, model_tags, fingerprint_overrides
```

---

## 2. Entità Principali & Schema SQLite M1

### 2.1 `sources` (Sorgente Locale)
Rappresenta una cartella radice locale registrata dall'utente in sola lettura (in M4 estesa con `kind = 'dropbox'`).

| Colonna | Tipo SQLite | Vincoli / Note |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Identificativo univoco |
| `kind` | `TEXT NOT NULL` | `CHECK (kind IN ('local_dir', 'dropbox'))` — in M1 solo `'local_dir'` |
| `name` | `TEXT NOT NULL` | Nome descrittivo visualizzato in sidebar |
| `root_uri` | `TEXT NOT NULL UNIQUE` | Percorso canonico normalizzato (es. `local://D:/Stampa3D`) |
| `cursor_token` | `TEXT` | NULL in M1 (usato da Dropbox in M4) |
| `status` | `TEXT NOT NULL DEFAULT 'idle'` | `CHECK (status IN ('idle', 'scanning', 'unavailable', 'error'))` |
| `last_error` | `TEXT` | Ultimo errore operativo di accesso radice |
| `last_scan_at` | `INTEGER` | Unix timestamp (secondi) ultimo completamento scansione |
| `created_at` | `INTEGER NOT NULL` | Unix timestamp registrazione sorgente |

**Regole di Validazione:**
- **Prevenzione Sovrapposizioni (FR-003):** Prima di inserire una nuova `local_dir`, il percorso canonico viene normalizzato (case-insensitive su Windows, slash `/` uniformati, rimozione trailing slash). È rifiutata se coincide con una sorgente esistente, è discendente di una sorgente esistente, oppure è antenata di una sorgente esistente.
- **Protezione Sorgente Irraggiungibile:** Se `root_uri` non è accessibile all'avvio scansione, `status` passa a `'unavailable'`, viene registrato `last_error` e nessun record `folders`/`models`/`files` della sorgente viene eliminato.

---

### 2.2 `folders` (Nodo Cartella Navigabile)
Rappresenta una cartella non-tecnica navigabile nella gerarchia della sorgente. Le sottocartelle puramente tecniche assorbite in un `folder_pack` (FR-007 / FR-011) **non** hanno riga in `folders`.

| Colonna | Tipo SQLite | Vincoli / Note |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Identificativo univoco |
| `source_id` | `INTEGER NOT NULL` | `REFERENCES sources(id) ON DELETE CASCADE` |
| `parent_id` | `INTEGER` | `REFERENCES folders(id) ON DELETE CASCADE` (`NULL` per radice sorgente) |
| `rel_path` | `TEXT NOT NULL` | Path relativo con `/` (`""` per la cartella radice della sorgente) |
| `name` | `TEXT NOT NULL` | Nome della cartella (per la riga radice `rel_path = ""` coincide con `sources.name`) |
| `model_count` | `INTEGER NOT NULL DEFAULT 0` | Modelli diretti nel livello corrente |
| `file_count` | `INTEGER NOT NULL DEFAULT 0` | File diretti nel livello corrente (incluse entry `.zip` dei modelli diretti) |
| `recursive_model_count` | `INTEGER NOT NULL DEFAULT 0` | Totale modelli nell'intero sotto-albero (incluso sé stesso) |
| `recursive_file_count` | `INTEGER NOT NULL DEFAULT 0` | Totale file nell'intero sotto-albero (incluse entry `.zip`) |
| `nested_folder_count` | `INTEGER NOT NULL DEFAULT 0` | Numero totale di sottocartelle non-tecniche discendenti nel sotto-albero |
| `sample_titles_json` | `TEXT` | Array JSON dei primi 3–4 `display_title` rappresentativi del sotto-albero |
| `hero_thumb_id` | `INTEGER` | Predisposto per M2 (`NULL` in M1) |
| `preview_json` | `TEXT` | Predisposto per M3 Smart Folder (`NULL` in M1) |
| `summary_json` | `TEXT` | JSON riepilogo estensioni/stati sul sotto-albero (es. `{"stl":42,"zip":3,"cloud":5}`; include sempre `"cloud"` per derivare `FolderCardDto.hasCloudPlaceholder = cloud > 0`) |
| *(vincolo)* | `UNIQUE(source_id, rel_path)` | Unicità del percorso relativo nella sorgente |

---

### 2.3 `models` (Modello / Pack — Unità Default UI)
Unità logica mostrata nella vista predefinita **Modelli / Pack**.

| Colonna | Tipo SQLite | Vincoli / Note |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Identificativo stabile preservato su spostamenti/rinomine |
| `source_id` | `INTEGER NOT NULL` | `REFERENCES sources(id) ON DELETE CASCADE` |
| `folder_id` | `INTEGER` | `REFERENCES folders(id) ON DELETE SET NULL` |
| `container_kind` | `TEXT NOT NULL` | `CHECK (container_kind IN ('folder_pack', 'zip_archive', 'single_file'))` |
| `canonical_uri` | `TEXT NOT NULL UNIQUE` | `local://...` (cartella pack, `.zip`, o file singolo) |
| `display_title` | `TEXT NOT NULL` | Titolo leggibile ripulito dal rumore tecnico (mai vuoto) |
| `raw_name` | `TEXT NOT NULL` | Nome grezzo originale cartella o file |
| `fingerprint` | `TEXT` | Hash `BLAKE3` aggregato per resilienza a spostamenti/rinomine (`NULL` quando nessun file membro residente possiede `content_hash`, es. `cloud_placeholder`) |
| `primary_file_id` | `INTEGER` | Riferimento al file principale rappresentativo (se applicabile) |
| `primary_ext` | `TEXT` | Estensione principale per badge rapido M1 (`zip`, `3mf`, `stl`, `lys`...) |
| `has_cloud_placeholder` | `INTEGER NOT NULL DEFAULT 0` | `1` se almeno un file o l'intero modello è segnaposto `cloud_placeholder` |
| `hero_thumb_id` | `INTEGER` | Predisposto per M2 (`NULL` in M1) |
| `file_count` | `INTEGER NOT NULL DEFAULT 0` | Numero file contenuti (es. tutte le entry supportate per `.zip` o `folder_pack`) |
| `total_bytes` | `INTEGER NOT NULL DEFAULT 0` | Somma dimensioni in byte |
| `updated_at` | `INTEGER NOT NULL` | Unix timestamp ultimo aggiornamento |

**Regole di Raggruppamento Deterministiche (FR-007) e `models.folder_id`:**
1. **`zip_archive`**: ogni file `.zip` rilevato (residente o placeholder), associato al `folder_id` della cartella che lo contiene.
2. **`folder_pack`**: cartella `P` che contiene (direttamente o tramite sottocartelle tecniche terminali assorbite `STL`, `STLs`, `Presupported`, `Pre-supported`, `Supported`, `Unsupported`, `Images`, `Renders`, `Render`, `Lychee`, `Files`, o scale `32mm`/`75mm`) almeno `2` file 3D/slicer non-`.zip`, oppure `1` file 3D/slicer + `>= 1` immagine supportata. Se `P` è una **cartella foglia** (non è la radice sorgente e non contiene ulteriori sottocartelle non-tecniche né `.zip`), `P` stessa viene rappresentata dal `folder_pack` con `models.folder_id = id(parent(P))` e **non** genera una riga `folders` separata (così navigando `parent(P)` l'utente vede direttamente le card dei modelli invece di cartelle mono-modello). Se invece `P` contiene anche sottocartelle non-tecniche o `.zip` (cartella mista), `P` mantiene la propria riga in `folders` e il pack dei file diretti usa `models.folder_id = id(P)`.
3. **`single_file`**: singolo file 3D/slicer isolato senza compagni né immagini nella stessa cartella, associato al `folder_id` della cartella contenitore (o `parent(P)` se l'intera cartella foglia contiene solo quel file + eventuali sottocartelle tecniche assorbite).

---

### 2.4 `files` (`CatalogFile` — Singolo File o Entry Interna `.zip`)
Rappresenta ogni file supportato scoperto su disco o nella Central Directory di uno `.zip` residente.

| Colonna | Tipo SQLite | Vincoli / Note |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Identificativo univoco |
| `model_id` | `INTEGER NOT NULL` | `REFERENCES models(id) ON DELETE CASCADE` |
| `source_id` | `INTEGER NOT NULL` | `REFERENCES sources(id) ON DELETE CASCADE` |
| `folder_id` | `INTEGER` | `REFERENCES folders(id) ON DELETE SET NULL` |
| `parent_archive_id` | `INTEGER` | `REFERENCES files(id) ON DELETE CASCADE` (valorizzato per `zip_entry`) |
| `uri` | `TEXT NOT NULL UNIQUE` | `local://...` oppure `zip://<parent_uri>#<internal_rel_path>` |
| `rel_path` | `TEXT NOT NULL` | Path relativo alla cartella modello o interno allo `.zip` |
| `filename` | `TEXT NOT NULL` | Nome file con estensione |
| `ext` | `TEXT NOT NULL` | Estensione normalizzata lowercase (`stl`, `3mf`, `lys`, `lyt`, `obj`, `ply`, `glb`, `step`, `stp`, `gcode`, `zip`, `jpg`, `jpeg`, `png`, `webp`) |
| `storage_state` | `TEXT NOT NULL` | `CHECK (storage_state IN ('local_resident', 'cloud_placeholder', 'dropbox_remote', 'zip_entry'))` |
| `size_bytes` | `INTEGER NOT NULL` | Dimensione in byte (non compressa per `zip_entry`) |
| `mtime_unix` | `INTEGER` | Timestamp ultima modifica su disco (secondi) |
| `content_hash` | `TEXT` | Hash `BLAKE3` esadecimale (solo `local_resident`; `NULL` per `cloud_placeholder`) |
| `zip_entry_offset` | `INTEGER` | Offset locale/header (predisposto per M2/M4) |
| `zip_entry_csize` | `INTEGER` | Dimensione compressa nello `.zip` |
| `zip_entry_method` | `INTEGER` | Metodo compressione (`0`=Store, `8`=Deflate...) |
| `triangle_count` | `INTEGER` | `NULL` in M1 (popolato in M2 Pass 3) |
| `bbox_x_mm` / `y` / `z` | `REAL` | `NULL` in M1 (popolato in M2 Pass 3) |
| `thumb_id` | `INTEGER` | `NULL` in M1 (popolato in M2) |
| `index_pass` | `INTEGER NOT NULL DEFAULT 1` | `1` = inventario veloce, `2` = hash + ZIP Central Directory |

**Macchina a Stati di `storage_state` e `index_pass` in M1:**
- Placeholder Win32 (`FILE_ATTRIBUTE_OFFLINE` o `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`): creato subito con `storage_state = 'cloud_placeholder'`, `content_hash = NULL`, `index_pass = 2` (nessuna apertura file né in Pass 1 né in Pass 2).
- File residente locale (`local_resident`):
  - Pass 1 $\rightarrow$ `index_pass = 1`, `content_hash = NULL` (oppure riusato se file esistente con stesso `(size_bytes, mtime_unix)`).
  - Pass 2 $\rightarrow$ se `(size_bytes, mtime_unix)` invariati e `content_hash` presente, skip hashing; altrimenti calcolo `BLAKE3` + ispezione Central Directory se `.zip`, poi `index_pass = 2`.
- Voci dentro `.zip` residente: create in Pass 2 con `storage_state = 'zip_entry'`, `parent_archive_id = <zip_file_id>`, `index_pass = 2`.

---

### 2.5 Tabelle Full-Text Search (`FTS5`)

```sql
CREATE VIRTUAL TABLE models_fts USING fts5(
    model_id UNINDEXED,
    title,       -- display_title + raw_name normalizzato
    path_tokens, -- segmenti del path relativo della cartella
    file_tokens, -- nomi file contenuti ed estensioni (inclusi zip_entry)
    tags_it_en,  -- vuoto in M1; popolato dal Motore Tassonomico in M3
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE files_fts USING fts5(
    file_id UNINDEXED,
    model_id UNINDEXED,
    filename,    -- filename + stem normalizzato
    rel_path,    -- path cartella + path interno eventuale zip
    ext,         -- estensione normalizzata (es. "stl", "3mf", "lys")
    tokenize = 'unicode61 remove_diacritics 2'
);
```

- **Indici B-Tree di supporto:**
  - `idx_folders_parent ON folders(source_id, parent_id)`
  - `idx_models_folder ON models(folder_id)`
  - `idx_models_source ON models(source_id)`
  - `idx_models_fingerprint ON models(fingerprint) WHERE fingerprint IS NOT NULL`
  - `idx_files_hash ON files(content_hash) WHERE content_hash IS NOT NULL`
  - `idx_files_model ON files(model_id)`
  - `idx_files_folder ON files(folder_id)`
  - `idx_files_ext ON files(ext)`
  - `idx_files_parent_archive ON files(parent_archive_id) WHERE parent_archive_id IS NOT NULL`

### 2.6 Riconciliazione Spostamenti e Sincronizzazione `FTS5`
- **Guardia `fingerprint` su placeholder:** Modelli composti solo da `cloud_placeholder` (privi di `content_hash`) mantengono `fingerprint = NULL` e non partecipano al matching per spostamento.
- **Riconciliazione atomica 1:1:** Durante l'aggiornamento incrementale di una sorgente accessibile, la riconciliazione di spostamento/rinomina viene applicata **solo** quando per uno stesso `fingerprint` non nullo esiste esattamente **1** modello scomparso e **1** nuovo modello apparso nello stesso run (evitando collisioni su copie identiche). In un'unica transazione SQLite atomica, l'eventuale riga provvisoria creata al nuovo `canonical_uri` in Pass 1 viene rimossa (con relativi `files` e voci `FTS5`) prima di aggiornare `canonical_uri`, `folder_id`, `raw_name` e `display_title` sul `models.id` preservato.
- **Sincronizzazione esplicita `FTS5`:** Poiché `models_fts` e `files_fts` sono tabelle virtuali standalone prive di trigger, ogni inserimento, aggiornamento di percorso/titolo, espansione `.zip` o cancellazione (`remove_source`, pruning orfani) esegue esplicitamente `DELETE` + `INSERT` su `models_fts`/`files_fts` dentro la stessa transazione del writer.
