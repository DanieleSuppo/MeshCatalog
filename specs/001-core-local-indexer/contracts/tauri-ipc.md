# Tauri IPC Contracts: Core Local Indexer (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26  
**Master Reference**: [docs/04-UI-UX-AND-IPC.md](../../docs/04-UI-UX-AND-IPC.md)

Questo documento definisce i contratti IPC (`invoke` commands ed eventi asincroni `listen`) esposti dal backend Rust Tauri v2 verso il frontend React/TypeScript per la Milestone M1. I nomi dei comandi principali e la firma di `SearchQuery`, `SearchResultPage`, `get_folder_tree`, `get_model_detail`, `add_local_source`, `start_index_scan` e degli eventi `indexer://*` sono allineati a `docs/04-UI-UX-AND-IPC.md`.

---

## 1. Tipi Condivisi (TypeScript <-> Rust `serde` camelCase)

```typescript
export type ViewMode = 'models' | 'files';
export type SourceFilter = 'all' | 'local' | 'dropbox';
export type SourceKind = 'local_dir' | 'dropbox';
export type SourceStatus = 'idle' | 'scanning' | 'unavailable' | 'error';
export type ContainerKind = 'folder_pack' | 'zip_archive' | 'single_file';
export type StorageState =
  | 'local_resident'
  | 'cloud_placeholder'
  | 'dropbox_remote'
  | 'zip_entry';

export interface SourceDto {
  id: number;
  kind: SourceKind;
  name: string;
  rootUri: string;
  rootDisplayPath: string;
  status: SourceStatus;
  lastError: string | null; // riepilogo diagnostico, anche con status 'idle'
  lastScanAt: number | null; // completamento; gli avvisi ZIP recuperabili non lo bloccano
  modelCount: number;
  fileCount: number;
}

export interface FolderTreeNode {
  id: number;
  sourceId: number;
  parentId: number | null;
  relPath: string;
  name: string;
  recursiveModelCount: number;
  recursiveFileCount: number;
  nestedFolderCount: number;
  children: FolderTreeNode[];
}

export interface FolderCardDto {
  id: number;
  sourceId: number;
  relPath: string;
  name: string;
  recursiveModelCount: number;
  recursiveFileCount: number;
  nestedFolderCount: number;
  sampleTitles: string[]; // primi 3-4 display_title rappresentativi del sotto-albero
  hasCloudPlaceholder: boolean;
}

export interface ModelCardDto {
  id: number;
  sourceId: number;
  folderId: number | null;
  containerKind: ContainerKind;
  canonicalUri: string;
  displayTitle: string;
  rawName: string;
  relativePath: string; // percorso relativo contestuale nella sorgente/sotto-albero
  primaryExt: string | null;
  hasCloudPlaceholder: boolean;
  fileCount: number;
  totalBytes: number;
  updatedAt: number;
}

export interface FileCardDto {
  id: number;
  modelId: number | null; // nullo solo per immagini non associate; non aprire l'Inspector modello
  sourceId: number;
  folderId: number | null;
  parentArchiveId: number | null;
  parentArchiveName: string | null;
  uri: string;
  relPath: string; // path relativo contestuale (incluso archivio se zip_entry)
  filename: string;
  ext: string;
  storageState: StorageState;
  sizeBytes: number;
  mtimeUnix: number | null;
}

export interface SearchQuery {
  text: string;
  viewMode: ViewMode;
  sourceFilter: SourceFilter;
  sourceId?: number | null;
  folderId: number | null;
  facets: Record<string, string[]>; // vuoto in M1; preservato per compatibilità M3
  limit: number;
  offset: number;
}

export interface SearchResultPage {
  totalModels: number; // conteggio totale nello scope attivo (sourceFilter, sourceId, folderId, text) per paginazione
  totalFiles: number;  // conteggio totale file nello scope attivo
  subfolders: FolderCardDto[]; // popolato quando text è vuoto (incluse cartelle radice sorgente se folderId === null)
  items: ModelCardDto[] | FileCardDto[]; // in Rust: enum untagged SearchItemDto (ModelCardDto usa relativePath, FileCardDto usa relPath)
  facetCounts: Record<string, Array<{ slug: string; labelIt: string; count: number }>>;
  queryTimeMs: number;
}

export interface ModelDetailDto {
  id: number;
  sourceId: number;
  sourceName: string;
  containerKind: ContainerKind;
  canonicalUri: string;
  displayTitle: string;
  rawName: string;
  primaryExt: string | null;
  hasCloudPlaceholder: boolean;
  fileCount: number;
  totalBytes: number;
  updatedAt: number;
  files: FileCardDto[]; // elenco completo ordinato (disco, sottocartelle tecniche assorbite o entry .zip)
}
```

---

## 2. Comandi Tauri (`#[tauri::command]`)

### 2.1 Gestione Sorgenti Locali (FR-001, FR-002, FR-003)

| Comando | Input | Output (`Result<T, IpcError>`) | Comportamento / Invarianti |
| :--- | :--- | :--- | :--- |
| `list_sources` | `—` | `SourceDto[]` | Restituisce tutte le sorgenti registrate con conteggi aggregati aggiornati. |
| `add_local_source` | `path: string, name?: string` | `number` (`sourceId`) | Normalizza `path`, verifica che esista ed è una directory leggibile, rifiuta con `IpcError::SourceOverlap` se identica, parent o child di una sorgente già registrata; salva in SQLite senza scrivere nulla dentro `path`. |
| `remove_source` | `sourceId: number` | `void` | Rimuove `source` e (via `ON DELETE CASCADE`) `folders`, `models`, `files` e relative voci `FTS5`; non tocca alcun file su disco utente. Annulla la prenotazione accodata senza attendere altre sorgenti; per un job già in esecuzione attende solo il rilascio della sorgente rimossa. |
| `pick_local_folder` | `—` | `string \| null` | Apre dialog nativo selezione cartella in sola lettura tramite `tauri-plugin-dialog`. |

### 2.2 Orchestrazione Indicizzazione Background (FR-004..FR-010)

| Comando | Input | Output (`Result<T, IpcError>`) | Comportamento / Invarianti |
| :--- | :--- | :--- | :--- |
| `start_index_scan` | `sourceId?: number` | `void` | Avvia in background (non bloccante) Pass 1 + Pass 2 sulla sorgente indicata o su tutte le sorgenti `local_dir`. Se una scansione è già attiva per la stessa sorgente, restituisce `IpcError::ScanAlreadyRunning`. |
| `cancel_index_scan` | `sourceId?: number` | `void` | Annulla subito le prenotazioni accodate selezionate. I job già in esecuzione si fermano al confine della transazione corrente. |
| `get_scan_status` | `—` | `ScanStatusDto` | Restituisce se c'è una scansione attiva, passata corrente, conteggi ed eventuali errori recuperabili recenti. |

```typescript
export interface ScanStatusDto {
  isRunning: boolean;
  activeSourceId: number | null;
  pass: 1 | 2 | null;
  phaseLabel: string | null;
  processed: number;
  total: number;
  currentItem: string | null;
  recentErrors: IndexerErrorEvent[];
}
```

### 2.3 Navigazione, Ricerca & Inspector (FR-011..FR-014)

| Comando | Input | Output (`Result<T, IpcError>`) | Comportamento / Invarianti |
| :--- | :--- | :--- | :--- |
| `get_folder_tree` | `sourceFilter: SourceFilter` | `FolderTreeNode[]` | Legge da SQLite solo i nodi navigabili (`is_visible = 1`), comprese le cartelle tecniche non assorbite, e costruisce l'albero. |
| `search_catalog` | `query: SearchQuery` | `SearchResultPage` | Legge solo SQLite (`WAL` + `FTS5`). **Caso 1 (`text.trim() === ""` e `folderId === null`):** vista radice "Tutte le sorgenti" (o filtrata su `sourceId`/`sourceFilter`) — restituisce in `subfolders` le cartelle radice delle sorgenti (`parent_id IS NULL` con conteggi ricorsivi e `sampleTitles`) e `items = []`. **Caso 2 (`text.trim() === ""` e `folderId != null`):** restituisce `subfolders` (sottocartelle dirette navigabili con `recursiveModelCount`, `recursiveFileCount`, `nestedFolderCount`, `sampleTitles`) + `items` diretti del livello. **Caso 3 (`text.trim() !== ""`):** `subfolders` è `[]` e la query utente viene sanitizzata e ogni token è passato come prefisso letterale `"token"*` (la punteggiatura non ha significato sintattico). Filtra ricorsivamente l'intero sotto-albero, anche se contiene nodi non navigabili durante Pass 1 (o tutte le sorgenti se `folderId === null`) su `models_fts` (`viewMode === 'models'`) o `files_fts` (`viewMode === 'files'`). |
| `get_model_detail` | `modelId: number` | `ModelDetailDto` | Restituisce i metadati del modello e l'elenco completo e ordinato dei file associati, con percorsi relativi interni. Nessun parsing mesh né decodifica immagini in M1. |

---

Le immagini non associate sono restituite solo in vista `files` e concorrono a `totalFiles`, anche con ricerca ricorsiva. La UI non deve chiamare `get_model_detail(null)`.

## 3. Formato Errori Strutturati (`IpcError`)

```typescript
export type IpcErrorCode =
  | 'SOURCE_NOT_FOUND'
  | 'SOURCE_UNAVAILABLE'
  | 'SOURCE_OVERLAP'
  | 'INVALID_PATH'
  | 'SCAN_ALREADY_RUNNING'
  | 'MODEL_NOT_FOUND'
  | 'DATABASE_ERROR'
  | 'INTERNAL_ERROR';

export interface IpcError {
  code: IpcErrorCode;
  message: string;
  details?: string;
}
```

---

## 4. Eventi Asincroni Backend $\rightarrow$ Frontend (`listen`)

Allineati a `docs/04-UI-UX-AND-IPC.md` §4.2:

1. **`indexer://progress`**
   ```typescript
   export interface IndexerProgressEvent {
     sourceId: number;
     pass: 1 | 2 | 3; // in M1 emette 1 e 2
     phaseLabel: string; // es. "Passata 1/2 — Inventario cartelle", "Passata 2/2 — Analisi ZIP e impronte"
     processed: number;
     total: number;
     currentItem: string;
   }
   ```
2. **`indexer://batch_updated`**
   ```typescript
   export interface IndexerBatchUpdatedEvent {
     sourceId: number;
     updatedFolderIds: number[];
     updatedModelIds: number[];
   }
   ```
3. **`indexer://error`**
   ```typescript
   export interface IndexerErrorEvent {
     sourceId: number;
     message: string;
     path?: string;
     recoverable: boolean;
   }
   ```
4. **`indexer://completed`** (estensione utile per chiusura pulita indicatore UI)
   ```typescript
   export interface IndexerCompletedEvent {
     sourceId: number | null;
     cancelled: boolean;
     durationMs: number;
     modelsIndexed: number;
     filesIndexed: number;
     errorsCount: number;
   }
   ```
