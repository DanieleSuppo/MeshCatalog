# 04 — Specifiche UI/UX e Contratti Tauri IPC (`MeshCatalog`)

## 1. Principi Visuali e Prestazionali

- **Budget di Rendering Griglia `< 16 ms`:** Virtualizzazione totale con `@tanstack/react-virtual`. Le card caricano esclusivamente stringhe da SQLite e immagini locali `.webp` pre-generate (`asset://` protocol Tauri).
- **Tema Studio Neutro Scuro:** Palette scura neutra ad alto contrasto (`#121417` sfondo, `#1B1F24` card, `#272C33` bordi) progettata per far risaltare le sculture in grigio argilla (clay/matcap) e i colori dei render `.3mf`.
- **Zero WebGL in Griglia:** Nessun canvas Three.js nella vista elenco; il motore 3D vive in un'unica istanza montata on-demand nell'Inspector o nel modale QuickLook.

---

## 2. Struttura dell'Interfaccia (4 Aree Principali)

```text
+------------------------------------------------------------------------------+
| [ Search: "pupazzetti fantasy" ]  [Modelli | Singoli File]  [⚙ Sync/Cloud]  |
+----------------------+-------------------------------------------------------+
| FONTI                | Breadcrumb: Dropbox / Resina / Artisan Guild          |
| [Tutti] [SSD] [Cloud]|-------------------------------------------------------+
|                      |                                                       |
| CARTELLE             |  [ Smart Folder Card ]   [ Model / Pack Card ]        |
| ▸ D:\Stampa3D (40GB) |  +--------+--+--+        +-------------------+        |
| ▾ Dropbox (660GB)    |  |        |T2|T3|        |   Hero / Render   |        |
|   ▸ Resina           |  |  Hero  +--+--+        |  (Hover = 4-view  |        |
|   ▸ FDM              |  |        |  T4 |        |     scrubbing)    |        |
|                      |  +--------+--+--+        +-------------------+        |
| FILTRI AUTOMATICI    |  Artisan Guild · 14 mod  King Thror (Dwarves)         |
| Tecnologia:          |  [Resina] [Fantasy]      [SSD] [Resina] [32mm]        |
|  [x] Resina (1.420)  +-------------------------------------------------------+
|  [ ] FDM (380)       | INSPECTOR / QUICKLOOK (collassabile / tasto Spazio)   |
| Categoria:           |  [ Viewport 3D On-Demand (Three.js) ]                 |
|  [x] Miniatura (980) |  Contenuto pack (22 STL, 1 LYS, 2 JPG)                |
|  [ ] Scenario (290)  |  Tag automatici: [Resina ×] [Fantasy ×] [+]           |
+----------------------+-------------------------------------------------------+
```

---

## 3. Comportamento dei Componenti Chiave

### 3.1 Smart Folder Card ("Vedere dentro prima di entrare")
Ogni cartella nella griglia espone immediatamente il proprio contenuto visivo:
- **Layout Bento (`1 Hero + 3 Thumbnails`):**
  1. **Selezione deterministica dell'immagine Hero** (in ordine di priorità):
     1. Foto render del creator trovata nella cartella o nei modelli figli immediati (`.jpg`/`.png`/`.webp`, escludendo loghi o `banner.jpg`).
     2. Anteprima `Metadata/plate_1.png` di un file `.3mf`.
     3. Anteprima `preview.png` estratta da un `.lys`.
     4. Render `wgpu` dell'`.stl` con **maggiore complessità geometrica** (`triangle_count * bounding_box_diagonal`), scartando automaticamente micro-pezzi di servizio (perni, viti, basette semplici `< 2.000` triangoli se esistono pezzi più ricchi).
  2. **3 Thumbnails secondarie:** Campionate dai modelli più diversi o rappresentativi della sottocartella.
- **Hover Scrubbing Cartella:** Muovendo il cursore orizzontalmente sopra la card della cartella, l'area preview scorre ciclicamente le anteprime dei modelli contenuti all'interno (fino a 12 modelli campione) mostrando il nome del modello correntemente inquadrato.
- **Badge di sintesi:** Numero modelli/file, tecnologie prevalenti rilevate (`Resina` / `FDM`) e indicatore origine (`SSD` / `Dropbox` / `Misto`).

### 3.2 Model / Pack Card (Vista Default)
- **Titolo pulito dedotto** (`display_title`) + sottotitolo discreto con studio/path breve.
- **Multi-Angle Hover Scrub:** Se la thumbnail nasce da `wgpu` (sprite `.webp` a 4 angolazioni), muovendo il mouse orizzontalmente sulla card ruota istantaneamente la vista del modello senza caricare alcuna mesh.
- **Badge stato & formato:** `SSD` (verde/neutro) · `Cloud` (azzurro Dropbox) · `ZIP` (se compresso) · `3MF` / `LYS` / `STL`.

### 3.3 Barra Superiore & Controlli Indicizzazione
- **Toggle `Modelli / Pack` (default) $\leftrightarrow$ `Singoli File`:** Cambia istantaneamente la granularità dei risultati mantenendo filtri e query attivi.
- **Pill Indicizzatore Background:** Mostra Passata corrente (`Passata 2/3 — Analisi ZIP`), barra progresso non bloccante e interruttore **`Genera render 3D in RAM per modelli Cloud privi di foto`**.

### 3.4 Inspector Laterale & QuickLook 3D (`Spazio`)
- **Viewport 3D On-Demand (`Three.js`):**
  - Attivato cliccando *"Carica vista 3D"* o premendo `Spazio` (con caricamento automatico se il file è locale su SSD o dentro uno `.zip` locale; per file su Dropbox chiede conferma esplicita *"Scarica temporaneamente in RAM per anteprima 3D"* mostrando la dimensione in MB).
  - Shader Studio Clay / Matcap neutra per esaltare i dettagli fini delle miniature + controllo rapido assi, dimensioni reali in `mm`, conteggio triangoli e toggle wireframe.
- **Immagini non associate:** In M1 compaiono solo in `Singoli File`, con `FileCardDto.modelId: number | null`. Se il valore è `null`, la UI non chiama `get_model_detail`.
- **Elenco File nel Pack:** Se il modello è una cartella o uno `.zip`, mostra tutti i file contenuti (`.stl`, `.3mf`, `.lys`, `.jpg`) permettendo di ispezionare un singolo pezzo in 3D.
- **Correzione Tag Inline:** Pillole tag con evidenza della motivazione al passaggio del mouse (es. *Dedotto da cartella "Resina"*), bottone `×` per escludere un tag errato e `+` per aggiungerne uno da quelli della tassonomia.

---

## 4. Contratti Tauri IPC (`Commands` & `Events`)

### 4.1 Comandi (`invoke`)

```typescript
export type ViewMode = 'models' | 'files';
export type SourceFilter = 'all' | 'local' | 'dropbox';

export interface SearchQuery {
  text: string;
  viewMode: ViewMode;
  sourceFilter: SourceFilter;
  folderId: number | null;
  facets: Record<string, string[]>; // es. { print_tech: ['resin'], theme: ['fantasy'] }
  limit: number;
  offset: number;
}

export interface SearchResultPage {
  totalModels: number;
  totalFiles: number;
  subfolders: FolderCardDto[];
  items: ModelCardDto[] | FileCardDto[];
  facetCounts: Record<string, Array<{ slug: string; labelIt: string; count: number }>>;
}

// Comandi principali esposti dal backend Rust
export interface MeshCatalogCommands {
  search_catalog(query: SearchQuery): Promise<SearchResultPage>;
  get_folder_tree(sourceFilter: SourceFilter): Promise<FolderTreeNode[]>;
  get_model_detail(modelId: number): Promise<ModelDetailDto>;
  override_model_tag(modelId: number, facet: string, slug: string, action: 'add' | 'exclude'): Promise<void>;
  load_mesh_bytes_for_viewer(fileId: number, allowRemoteFetch: boolean): Promise<Uint8Array>;
  start_index_scan(sourceId?: number): Promise<void>;
  set_cloud_ram_render_enabled(enabled: boolean): Promise<void>;
  connect_dropbox_oauth(): Promise<{ accountEmail: string }>;
  add_local_source(path: string, name?: string): Promise<number>;
}
```

### 4.2 Eventi Asincroni Backend $\rightarrow$ Frontend (`listen`)

- `indexer://progress`: `{ pass: 1 | 2 | 3, phaseLabel: string, processed: number, total: number, currentItem: string }`
- `indexer://batch_updated`: `{ updatedFolderIds: number[], updatedModelIds: number[] }` — permette alla UI di aggiornare solo le thumbnail o i conteggi visibili senza ricaricare l'intera griglia.
- `indexer://error`: `{ sourceId: number, message: string, recoverable: boolean }`
