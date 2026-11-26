# AGENTS.md — Contesto Autorevole di MeshCatalog

> Leggere questo file all'inizio di ogni sessione prima di progettare, modificare codice o proporre architetture.

## 1. Identità e Ambiente del Progetto

- **Nome progetto:** `MeshCatalog`
- **Obiettivo:** Applicazione desktop **Windows (10/11)** 100% locale e gratuita per trovare, catalogare automaticamente e visualizzare molto velocemente grandi collezioni di file per stampa 3D (~700 GB distribuiti tra SSD interno e Dropbox, inclusi archivi `.zip`).
- **Stack scelto:** **Tauri v2** (Backend **Rust** + **SQLite `FTS5`** + headless renderer **`wgpu`**) e Frontend **React + TypeScript + Vite + Three.js (`@react-three/fiber`)**.
- **Workflow di sviluppo cross-OS:**
  - Il codice e le specifiche vengono sviluppati e committati da **WSL2 (Linux)**.
  - Il repository GitHub (`DanieleSuppo/MeshCatalog`) funge da ponte verso **Windows**, dove viene eseguito `git pull`, compilato e testato il binario Tauri target (`x86_64-pc-windows-msvc`).

---

## 2. Invarianti Architetturali Non Negoziabili

Qualsiasi modifica futura **deve rispettare** questi 4 vincoli strutturali:

1. **Read-Only sui File Utente (v1):**
   - L'indicizzatore e la UI **non devono mai** modificare, spostare, rinominare o scrivere file nascosti/sidecar dentro le cartelle dei modelli 3D dell'utente.
   - Tutto lo stato applicativo (database SQLite, cache miniature `.webp`, log e token OAuth locali) risiede esclusivamente nella directory dati dell'applicazione (`%LOCALAPPDATA%\MeshCatalog`).
2. **Dropbox Anti-Hydration Guard (Zero-Explosion):**
   - Su Windows, prima di aprire *qualsiasi* file su disco per hashing, parsing o estrazione ZIP, verificare gli attributi Win32 tramite `std::fs::metadata` / `GetFileAttributesW` **senza aprire file handle**:
     - `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (`0x00400000`)
     - `FILE_ATTRIBUTE_OFFLINE` (`0x00001000`)
   - Se uno dei due bit è attivo, il file è un segnaposto *Online-Only*: **è severamente vietato chiamare `File::open` o invocare API Shell di Windows per le thumbnail**, perché forzerebbero Dropbox a scaricare centinaia di GB sul disco locale.
   - I file remoti su Dropbox vengono ispezionati esclusivamente tramite le API Dropbox (`list_folder`, `get_thumbnail_batch`, `HTTP Range` sulla Central Directory degli `.zip`/`.3mf`).
3. **Zero-Latency Browsing (Separazione 2D / 3D):**
   - La navigazione nelle griglie, alberi cartelle, ricerche e filtri legge **esclusivamente** SQLite e la cache locale di miniature `.webp`. Budget target per frame di scorrimento: **`< 16 ms`**.
   - **Vietato** istanziare contesti WebGL/Three.js o parsare mesh 3D dentro le card della griglia. Il viewer 3D interattivo viene montato **solo su richiesta** nell'Inspector laterale o nel QuickLook (`Spazio`).
4. **Tassonomia Automatica Guidata dal Codice (No Configurator UI):**
   - Non esiste un flusso di tagging manuale esteso né un pannello UI per costruire regole o filtri personalizzati.
   - Le regole semantiche, i sinonimi bilingui IT/EN e i dizionari di classificazione vivono **nel codice/configurazione versionata del repository**.
   - La UI espone solo una correzione rapida inline dei tag su un singolo modello (`origin = 'user_override'`, inclusa la soppressione di tag errati via `is_excluded = 1`), che sopravvive a re-indicizzazioni e spostamenti grazie al tracciamento tramite hash (`BLAKE3` / Dropbox `content_hash`).

---

## 3. Perimetro v1 vs Roadmap v2+

- **Dentro v1:** Scansione multi-passaggio locale SSD + Dropbox API (`HTTP Range` per `.zip`/`.3mf`), interruttore opzionale per rendering 3D in RAM volatile di `.stl` su cloud privi di foto, vista **Modelli/Pack (default)** con toggle **Singoli File**, Smart Folder Preview (*1 Hero + 3 Thumbnails* + *Hover Scrub*), Motore Tassonomico automatico bilingue, tracking spostamenti tramite hash.
- **Fuori v1 (rimandato a v2+):** Gestione e pulizia duplicati, integrazione "Apri in Bambu Studio / Lychee Slicer", tagging manuale libero, classificazione visiva locale offline (CLIP/ONNX) per pacchetti privi di testo significativo.

---

## 4. Mappa della Documentazione

| Documento | Contenuto |
| :--- | :--- |
| [docs/01-PRODUCT-SPEC.md](./docs/01-PRODUCT-SPEC.md) | Profilo collezione, matrice formati a costo zero, UX Modelli vs File, perimetro v1/v2+. |
| [docs/02-ARCHITECTURE.md](./docs/02-ARCHITECTURE.md) | Stack Tauri v2/Rust/React, architettura Dropbox Zero-Explosion, pipeline 3 passate e schema SQLite DDL. |
| [docs/03-TAXONOMY-ENGINE.md](./docs/03-TAXONOMY-ENGINE.md) | Algoritmo di classificazione multi-segnale, facce ortogonali, dizionario IT/EN e regole di override. |
| [docs/04-UI-UX-AND-IPC.md](./docs/04-UI-UX-AND-IPC.md) | Specifiche UI, algoritmo Smart Folder Preview, viewport 3D on-demand e contratti Tauri IPC. |
| [docs/05-ROADMAP.md](./docs/05-ROADMAP.md) | Piano di implementazione per milestone (`M0` -> `M5`) e checklist build WSL2 <-> Windows. |

---

## 5. Workflow Spec Kit (Spec-Driven Development)

Lo sviluppo delle milestone segue il workflow **GitHub Spec Kit** (`specify`, integrazione Antigravity in `.agents/skills/speckit-*` e infrastruttura in `.specify/`):

- **Costituzione di progetto:** [Spec Kit constitution](./.specify/memory/constitution.md) — codifica gli invarianti non negoziabili (Principi I–V) validati ad ogni gate `/speckit-plan` e `/speckit-analyze`.
- **Specifiche operative di milestone:** [Working specifications](./specs) — artefatti Spec Kit per ciascuna milestone (`spec.md`, `plan.md`, `tasks.md`).
- **Architettura Master vs Specifiche di Milestone:**
  - `docs/01..05` contengono la specifica master globale (DDL SQLite completo, matrice formati, contratti IPC, roadmap `M1`–`M5`).
  - `specs/NNN-<milestone>/` ospita gli artefatti operativi generati per ciascuna milestone (`/speckit-specify` $\rightarrow$ `/speckit-plan` $\rightarrow$ `/speckit-tasks` $\rightarrow$ `/speckit-analyze` $\rightarrow$ `/speckit-implement`).
- **Granularità:** Usare Spec Kit per verticali/milestone complete (`M1`..`M4`), non per micro-modifiche ai dizionari o correzioni puntuali.

