# AGENTS.md — Contesto Autorevole di MeshCatalog

> Leggere questo file all'inizio di ogni sessione prima di progettare, modificare codice o proporre architetture.

## 1. Identità e Ambiente del Progetto

- **Nome progetto:** `MeshCatalog`
- **Obiettivo:** Applicazione desktop **Windows (10/11)** 100% locale e gratuita per trovare, catalogare automaticamente e visualizzare molto velocemente grandi collezioni di file per stampa 3D (~700 GB distribuiti tra SSD interno e Dropbox, inclusi archivi `.zip`).
- **Stack scelto:** **Tauri v2** (Backend **Rust** + **SQLite `FTS5`** + headless renderer **`wgpu`**) e Frontend **React + TypeScript + Vite + Three.js (`@react-three/fiber`)**.
- **Workflow di sviluppo cross-OS:**
  - **WSL2 (Linux):** sviluppo, progettazione Spec Kit, verifica del crate puro `meshcatalog-core` (`cargo fmt --check`, `cargo clippy --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core --all-targets -- -D warnings`, `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core`) e test/build frontend (`npm test`, `npm run build` con **Node.js LTS `>= 22.12.0`**). Il crate binario `meshcatalog-app` (GUI Tauri) viene compilato e testato direttamente sul target nativo Windows senza richiedere WebKitGTK su WSL2.
  - **Ponte GitHub (`DanieleSuppo/MeshCatalog`) -> Windows 10/11 (`x86_64-pc-windows-msvc`):** su Windows clonare e compilare **sempre su NTFS locale fuori da cartelle sincronizzate Dropbox** (es. `C:\Dev\MeshCatalog` o `C:\Users\<utente>\MeshCatalog\git`) per evitare file-lock `EBUSY` su `node_modules` e `src-tauri\target`.

---

## 2. Invarianti Architetturali Non Negoziabili

Qualsiasi modifica futura **deve rispettare** questi 4 vincoli strutturali:

1. **Read-Only sui File Utente (v1):**
   - L'indicizzatore e la UI **non devono mai** modificare, spostare, rinominare o scrivere file nascosti/sidecar dentro le cartelle dei modelli 3D dell'utente.
   - Tutto lo stato applicativo (database SQLite, cache miniature `.webp`, log e token OAuth locali) risiede esclusivamente nella directory dati dell'applicazione (`%LOCALAPPDATA%\MeshCatalog`).
2. **Dropbox Anti-Hydration Guard (Zero-Explosion):**
   - Prima di enumerare cartelle o aprire file su Windows, verificare attributi e tag reparse con lookup Win32 di soli metadati, inclusi radice e antenati. Vedere [guard Win32](./docs/02-ARCHITECTURE.md#21-scudo-anti-idratazione-su-disco-locale-win32-guard).
   - Con `OFFLINE`, `RECALL_ON_OPEN` o `RECALL_ON_DATA_ACCESS`, non enumerare la cartella e non chiamare `File::open` né API Shell per le thumbnail.
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

Lo sviluppo delle milestone segue il workflow **GitHub Spec Kit** (`specify`, integrazione in `.agents/skills/speckit-*` e infrastruttura in `.specify/`):

- **Costituzione di progetto:** [Spec Kit constitution](./.specify/memory/constitution.md) — codifica gli invarianti non negoziabili (Principi I–V) validati ad ogni gate `/speckit-plan` e `/speckit-analyze`.
- **Specifiche operative di milestone:** [Working specifications](./specs) — artefatti Spec Kit per ciascuna milestone (`spec.md`, `plan.md`, `tasks.md`).
- **Architettura Master vs Specifiche di Milestone:**
  - `docs/01..05` contengono la specifica master globale (DDL SQLite completo, matrice formati, contratti IPC, roadmap `M1`–`M5`).
  - `specs/NNN-<milestone>/` ospita gli artefatti operativi generati per ciascuna milestone (`/speckit-specify` $\rightarrow$ `/speckit-clarify` $\rightarrow$ `/speckit-plan` $\rightarrow$ `/speckit-tasks` $\rightarrow$ `/speckit-analyze` $\rightarrow$ **STOP** $\rightarrow$ `/speckit-implement`).
- **Granularità:** Usare Spec Kit per verticali/milestone complete (`M1`..`M4`), non per micro-modifiche ai dizionari o correzioni puntuali.

### 5.1 Fase di Design Spec Kit (Pre-Implementazione)

Quando si progetta una nuova milestone (`M1`..`M4`), seguire rigorosamente questa sequenza:

1. **Setup branch e puntatore locale:** partire da `main` aggiornato, creare il branch `NNN-<milestone>` e impostare `.specify/feature.json` su `{"feature_directory": "specs/NNN-<milestone>"}` (file locale in `.gitignore`).
2. **Un subagente isolato per ogni step con bootstrap `/ctx`:** eseguire ciascuno step di design in un **subagente dedicato a contesto isolato** (in Antigravity: `invoke_subagent` con `TypeName: "self"`). Ogni subagente **deve come primo passo** inizializzare il contesto tramite la skill `ctx` (`$ctx` / `/ctx`: verifica `.agents/ctx.cache.md` su `git rev-parse HEAD` e, in caso di cache miss, delega la scansione *read-only* a un subagente con modello economico `flash`, mai ereditando silenziosamente il modello principale) e poi eseguire la skill Spec Kit dello step:
   - **Step 1 — `/speckit-specify`:** genera `specs/NNN-<milestone>/spec.md` e `checklists/requirements.md` (`16/16` PASS).
   - **Step 2 — `/speckit-clarify` (Gate Interattivo Obbligatorio):** individua solo le ambiguità non già risolte in `AGENTS.md`, nella Costituzione o in `docs/01..05` e pone all'utente **una sola domanda alla volta** in italiano (max 5). **Vietato** procedere a `/speckit-plan` finché `/speckit-clarify` non è completato e le risposte dell'utente non sono registrate in `spec.md` (salvo delega esplicita dell'utente ad applicare le opzioni raccomandate per le domande restanti).
   - **Step 3 — `/speckit-plan`:** produce `plan.md`, `research.md`, `data-model.md`, `contracts/` e `quickstart.md`, superando il *Constitution Check* (Principi I–V) pre- e post-design.
   - **Step 4 — `/speckit-tasks`:** genera `tasks.md` ordinato per fasi, dipendenze e User Story con copertura 100% dei requisiti `FR-*` / `SC-*`.
   - **Step 5 — `/speckit-analyze`:** esegue la verifica *read-only* di allineamento tra `spec.md`, `plan.md`, `tasks.md`, Costituzione e codebase reale; correggere eventuali rilievi `CRITICAL`, `HIGH` o `MEDIUM` e ripetere il re-check *read-only* fino a ottenere **`0 CRITICAL / 0 HIGH / 0 MEDIUM`**.
3. **Stop obbligatorio prima di `/speckit-implement`:** fare commit e push sul branch `NNN-<milestone>` dei soli artefatti in `specs/NNN-<milestone>/` e **fermarsi senza modificare `src/` o `src-tauri/`**, attendendo che l'utente avvii esplicitamente la fase di implementazione (nella stessa sessione o in un harness dedicato).

### 5.2 Fase di Implementazione, Peer Review e Gate Nativo Windows

1. **Implementazione (`/speckit-implement`):** all'apertura della sessione di implementazione leggere `AGENTS.md` (o eseguire `$ctx`), verificare che `.specify/feature.json` punti a `specs/NNN-<milestone>` (ricreandolo se assente poiché gitignored) ed eseguire i task seguendo `tasks.md`, `plan.md` e `contracts/`.
2. **Gate automatici WSL2 + Peer Review:** completata l'implementazione, eseguire tutti i check WSL2 (`cargo fmt --check`, `cargo clippy -D warnings` e `cargo test` su `meshcatalog-core`, `npm test`, `npm run build`) seguiti da una **peer review** approfondita del diff rispetto a specifica e invarianti (`ponytail-review`), risolvendo ogni rilievo prima del test Windows.
3. **Gate nativo Windows 10/11 e chiusura milestone:** fare push sul branch e validare su Windows nativo `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --target x86_64-pc-windows-msvc`, `npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked` e gli scenari reali di `quickstart.md` (**0 byte** scaricati sui segnaposto Dropbox *Online-Only* e rispetto dei budget prestazionali). Registrare l'evidenza in `quickstart.md`, marcare i task completati in `tasks.md`, impostare `Status: Implemented` in `spec.md`, aggiornare `docs/05-ROADMAP.md` ed effettuare il merge della PR in `main` **solo su conferma esplicita dell'utente**.

