# Implementation Plan: Core Local Indexer & Baseline Browser (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26 | **Spec**: [spec.md](./spec.md)  
**Input**: Feature specification from [specs/001-core-local-indexer/spec.md](./spec.md)

## Summary

La Milestone M1 costruisce le fondamenta desktop di **MeshCatalog** con **Tauri v2** (backend **Rust** + **SQLite `WAL` / `FTS5`**) e **React 19 + TypeScript + Vite**:
- Gestione sorgenti locali in sola lettura con stato persistito esclusivamente in `%LOCALAPPDATA%\MeshCatalog` (zero sidecar nelle cartelle utente).
- Scanner locale NTFS dotato di **Win32 Anti-Hydration Guard** (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` `0x0040_0000` e `FILE_ATTRIBUTE_OFFLINE` `0x0000_1000`), esclusione automatica di file/cartelle di servizio, metadati macOS (`__MACOSX/`, `._*`) e mancato attraversamento di symlink/junction NTFS, pur catalogando i segnaposto cloud *Online-Only* dai soli metadati di nome/percorso con badge `Cloud / Online-Only`.
- Raggruppamento deterministico in `folder_pack` (con assorbimento delle sottocartelle tecniche terminali), `zip_archive` (lettura sola Central Directory tramite crate `zip` senza estrazione su disco) e `single_file`, con derivazione di `display_title` leggibili e scansione incrementale `BLAKE3` resiliente a spostamenti e rinomine.
- Shell UI desktop con tema studio neutro scuro (`#121417` / `#1B1F24` / `#272C33`), barra comandi/ricerca con switch istantaneo `Modelli / Pack (default) | Singoli File`, indicatore scansione non bloccante, albero delle cartelle navigabili, griglia virtualizzata (`@tanstack/react-virtual`) con anteprime ricorsive delle sottocartelle e Inspector laterale testuale di sola lettura (zero WebGL/3D o decodifica immagini in M1).

---

## Technical Context

**Language/Version**: Rust (stable, Edition 2021) per core engine e backend Tauri; TypeScript ~5.x + React 19 per frontend (`Node.js 22 LTS >=22.12.0`, verificato `v22.23.2` / `npm 10.9.8` in WSL2, `engines` in `package.json`; toolchain Rust da installare/verificare in WSL2 e su Windows MSVC).
**Primary Dependencies**:
- Backend Rust (`src-tauri/` workspace): `tauri` v2, `tauri-plugin-dialog`, `rusqlite` (`bundled`, `FTS5`), `blake3`, `zip` (solo lettura Central Directory), `windows-sys` (solo Windows), `tokio`, `tokio-util`, `rayon`, `serde`, `serde_json`, `thiserror`, `regex` (o parser leggero per scale/noise).
- Frontend UI (`src/`): `react` 19, `react-dom` 19, `@tanstack/react-virtual`, `@tauri-apps/api` v2, Vite.
**Storage**: SQLite locale (`%LOCALAPPDATA%\MeshCatalog\catalog.db`, modalità `WAL`, `foreign_keys = ON`, tabelle virtuali `FTS5` `models_fts` e `files_fts`) dove in M1 risiedono sorgenti e catalogo (`config.json` rimandato a M2/M4); zero scritture nelle directory utente.  
**Testing**:
- **WSL2 (Linux):** `cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core` (unit & integration test di grouping, esclusione `__MACOSX`/`._*`, mock Win32 placeholder bitmask, Central Directory `.zip`, hashing incrementale, move matching, benchmark FTS5 50k+ voci) + `npm run build` (`tsc --noEmit` + Vite).
- **Windows 10/11 (`x86_64-pc-windows-msvc`):** compilazione e test `meshcatalog-app` con lockfile prima degli scenari nativi T047 descritti in `quickstart.md`.
**Target Platform**: Desktop Windows 10/11 (`x86_64-pc-windows-msvc`), sviluppato cross-OS da WSL2 Linux via GitHub (`DanieleSuppo/MeshCatalog`).  
**Project Type**: Desktop Application (`Tauri v2`: workspace Rust backend + SPA React/TypeScript frontend).  
**Performance Goals**:
- Prima scansione ~40 GB locali + `.zip`: scoperta iniziale in pochi secondi, completamento Pass 1+2 in **< 60 s** senza bloccare la UI (SC-003).
- Ri-scansione incrementale invariata su ~40 GB: **< 5 s** con skip del **100%** degli hash invariati (SC-004).
- Ricerca testuale FTS5 e cambio vista `Modelli / Pack` $\leftrightarrow$ `Singoli File` su 50.000+ voci: **< 100 ms** (SC-006).
- Scorrimento griglia virtualizzata: frame budget **< 16 ms** (Principio III).  
**Constraints**:
- **0 byte** scaricati dal cloud su file con attributi `FILE_ATTRIBUTE_OFFLINE` o `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (Principio II / SC-001).
- **0 file** creati, modificati o eliminati dentro le directory dell'utente (Principio I / SC-002).
- Nessun contesto WebGL/Three.js e nessuna decodifica miniature in M1; zero estrazione `.zip` su disco; zero librerie con licenze proprietarie (Principio V).  
**Scale/Scope**: Progettato per oltre 50.000–200.000+ record file (`~40 GB` SSD locale + centinaia di GB di segnaposto *Online-Only* censiti dai soli metadati in M1, espandibili a ~700 GB completi con il connettore M4).

---

## Constitution Check

*GATE: Valutazione Pre-Design (Phase 0) e Post-Design (Phase 1) rispetto a [MeshCatalog Constitution](../../.specify/memory/constitution.md).*

| Principio | Stato Pre-Design | Stato Post-Design | Verifica e Controlli Specifici in M1 |
| :--- | :---: | :---: | :--- |
| **I. Read-Only User Filesystem (Zero Sidecars)** | PASS | PASS | Lo scanner apre i soli file residenti locali in sola lettura (`File::open`), non scrive mai `.zip` estratti né sidecar nelle cartelle utente; `catalog.db` e log risiedono esclusivamente in `app_local_data_dir` (`%LOCALAPPDATA%\MeshCatalog`). |
| **II. Dropbox Zero-Explosion (Win32 Anti-Hydration Guard)** | PASS | PASS | `EntryMetadataInspector` usa lookup Win32 di soli metadati per verificare attributi e tag reparse su radice, antenati e file. Il [guard Win32](../../docs/02-ARCHITECTURE.md#21-scudo-anti-idratazione-su-disco-locale-win32-guard) blocca enumerazione e aperture dati per i segnaposto. |
| **III. Zero-Latency 2D Browsing vs On-Demand 3D Viewport** | PASS | PASS | La UI legge solo da SQLite (`WAL` + `FTS5`) tramite connessioni read-only dedicate; griglia e lista usano `@tanstack/react-virtual` (`< 16 ms`). Zero WebGL/Three.js e zero parsing 3D in M1; l'Inspector mostra solo metadati testuali e lista file. |
| **IV. Code-Driven Automatic Bilingual Taxonomy (No Rule-Builder UI)** | PASS | PASS | M1 non introduce alcun tagging manuale o configuratore UI di regole; la pulizia `display_title` e il raggruppamento pack sono 100% deterministici nel codice Rust, e lo schema SQLite include già `tags`, `model_tags`, `fingerprint_overrides` e `models_fts` pronti per il motore tassonomico M3 senza migrazioni distruttive. |
| **V. Zero Proprietary Licenses & Cross-OS WSL2 <-> Windows Verification** | PASS | PASS | Solo crate/pacchetti open-source (`rusqlite` bundled, `blake3`, `zip`, `windows-sys`, `tokio`, `rayon`, React, `@tanstack/react-virtual`). La separazione in `meshcatalog-core` consente `cargo test` nativo in WSL2 Linux mentre l'app Tauri/Win32 viene verificata su Windows 10/11. |

La migrazione SQLite v2 rende opzionale `files.model_id` solo per le immagini non associate. Conserva ID, riferimenti, indici, FTS e override esistenti. L'interfaccia espone questi file senza Inspector modello. La migrazione v3 aggiunge `folders.is_visible`: i batch di inventario mantengono ricercabili i file senza mostrare candidati tecnici provvisori.

Nessuna violazione della Costituzione rilevata: la sezione *Complexity Tracking* resta vuota.

---

## Project Structure

### Documentation (this feature)

```text
specs/001-core-local-indexer/
├── spec.md                    # Specifica funzionale e chiarificazioni (input)
├── plan.md                    # Questo file (/speckit-plan)
├── research.md                # Decisioni tecniche e alternative valutate
├── data-model.md              # Schema SQLite DDL M1, relazioni, macchine a stati
├── quickstart.md              # Scenari di verifica WSL2 + Windows 10/11
└── contracts/
    └── tauri-ipc.md           # Contratti Tauri IPC (Commands, DTOs, Events)
```

### Source Code (repository root)

```text
package.json                   # Dipendenze React 19, Vite, @tanstack/react-virtual, @tauri-apps/api
index.html
vite.config.ts
tsconfig.json

src/                           # Frontend React + TypeScript (UI M1)
├── main.tsx
├── App.tsx                    # Layout 4 aree + stato navigazione & listener eventi IPC
├── styles/
│   └── theme.css              # Tema studio neutro scuro (#121417 / #1B1F24 / #272C33)
├── api/
│   └── tauriClient.ts         # Wrapper tipizzato invoke/listen allineato a contracts/tauri-ipc.md
├── components/
│   ├── TopCommandBar.tsx      # Search FTS5, toggle Modelli/Pack | Singoli File, progress pill
│   ├── SourceSidebar.tsx      # Elenco sorgenti locali + albero cartelle navigabili
│   ├── FolderCard.tsx         # Card sottocartella (conteggi ricorsivi, sottolivelli, 3-4 titoli)
│   ├── CatalogGrid.tsx        # Griglia/lista virtualizzata (@tanstack/react-virtual)
│   ├── ModelOrFileCard.tsx    # Card segnaposto M1 (badge formato, ZIP, Cloud / Online-Only)
│   └── TextInspectorPanel.tsx # Inspector laterale testuale di sola lettura (get_model_detail)
└── types/
    └── catalog.ts             # Tipi DTO TypeScript allineati a contracts/tauri-ipc.md

src-tauri/                     # Cargo Workspace Rust
├── Cargo.toml                 # Workspace members: [".", "crates/meshcatalog-core"]
├── tauri.conf.json
├── capabilities/
│   └── default.json           # Permessi Tauri v2 (core + dialog)
├── src/                       # Crate binario Tauri (`meshcatalog-app`)
│   ├── main.rs
│   ├── lib.rs                 # Bootstrap Tauri, app_local_data_dir, setup DB & stato
│   ├── commands.rs            # #[tauri::command] IPC handlers
│   └── scanner_service.rs     # Orchestrazione tokio + CancellationToken + emit eventi IPC
└── crates/
    └── meshcatalog-core/      # Crate pure-Rust testabile su WSL2 Linux & Windows
        ├── Cargo.toml
        ├── migrations/
        │   ├── 001_initial_schema.sql
        │   └── 002_allow_unassociated_images.sql
        ├── src/
        │   ├── lib.rs
        │   ├── contracts.rs   # DTO structs serde camelCase condivisi con IPC
        │   ├── error.rs       # Errori tipizzati dominio + codici IPC
        │   ├── db/
        │   │   ├── mod.rs     # Pool reader + writer + pragma WAL/FK
        │   │   ├── migrations.rs
        │   │   ├── sources.rs # CRUD sorgenti + check sovrapposizioni path
        │   │   ├── queries.rs # get_folder_tree, search_catalog (FTS5 + subtree), get_model_detail
        │   │   └── writer.rs  # Bulk upsert transazionale Pass 1 / Pass 2 + move reconciliation
        │   ├── scanner/
        │   │   ├── mod.rs
        │   │   ├── attr_guard.rs  # Trait MetadataInspector + Win32 Anti-Hydration Guard + test doubles
        │   │   ├── exclusions.rs  # Filtri cartelle/file di sistema, __MACOSX/, ._* e estensioni supportate
        │   │   ├── walker.rs      # Traversata: guardia symlink, reparse directory e placeholder
        │   │   ├── grouping.rs    # Regole folder_pack / zip_archive / single_file + assorbimento tecnico
        │   │   └── title.rs       # Derivazione deterministica display_title
        │   ├── archive/
        │   │   └── zip_reader.rs  # Lettura sola Central Directory .zip residente + error recovery
        │   ├── hasher/
        │   │   └── blake3_hasher.rs # Streaming BLAKE3 + skip (size, mtime) + fingerprint modello
        │   └── pipeline/
        │       └── mod.rs     # Esecuzione Pass 1 + Pass 2 con progress/batch/error sink
        └── tests/
            ├── anti_hydration_tests.rs
            ├── grouping_and_exclusions_tests.rs
            ├── zip_and_incremental_tests.rs
            └── fts5_perf_50k_tests.rs
```

**Structure Decision**: Adottato lo split **Cargo Workspace (`meshcatalog-core` + `src-tauri` Tauri app) + Frontend Vite React/TypeScript (`src/`)**. Questa separazione isola tutta la logica di indicizzazione, SQLite `FTS5`, lettura `.zip`, hash `BLAKE3` e Win32 guard mockabile in un crate `meshcatalog-core` verificabile via `cargo test` direttamente in WSL2, mantenendo puliti i confini Tauri IPC per il build nativo Windows.

---

## Complexity Tracking

*Nessuna violazione della Costituzione presente: tabella non applicabile.*
