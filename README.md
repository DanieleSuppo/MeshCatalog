# MeshCatalog

**Catalogatore desktop locale, organizzatore semantico automatico e visualizzatore ultra-veloce per collezioni di stampa 3D su Windows.**

MeshCatalog trasforma centinaia di gigabyte di modelli sparsi tra **SSD locale**, **Dropbox** e archivi **`.zip`** in un catalogo istantaneo, ricercabile in italiano e inglese, con anteprime intelligenti delle cartelle e zero connessioni cloud di terze parti.

---

## Pilastri del Progetto

- **Navigazione 2D a Latenza Zero:** Scorrimento fluido di decine di migliaia di modelli tramite database locale **SQLite (`FTS5`)** e cache di miniature **`.webp`**. Il rendering 3D interattivo viene attivato *solo su richiesta*.
- **Indicizzazione Multi-Passaggio in Background:** Scansione iniziale di nomi e alberatura in pochi secondi, seguita da estrazione miniature, classificazione semantica e rendering headless off-screen in background.
- **Smart Folder Preview (*Vedere dentro prima di entrare*):** Ogni cartella mostra un mosaico *1 Hero + 3 Thumbnails* scegliendo automaticamente render dei creator o i pezzi più significativi (scartando viti, perni o basette minuscole), più *Hover Scrub* al passaggio del mouse.
- **Tassonomia Semantica 100% Automatica (senza tagging manuale):** Deduce tecnologia (`Resina` / `FDM`), categoria (`Miniatura`, `Scenario`, `Mostro/Veicolo`, `Busto`, `Funzionale`), tema (`Fantasy`, `Sci-Fi`, `Grimdark`, `Storico`, ecc.), scala e supporti combinando ereditarietà dal path, euristiche sui file e dizionari bilingui IT/EN.
- **Architettura Dropbox "Zero-Explosion":** Progettato per gestire ~660 GB su Dropbox senza mai forzare il download su SSD. Rileva i placeholder Windows *Online-Only* senza idratarli e legge il contenuto e le copertine dentro gli `.zip`/`.3mf` remoti tramite `HTTP Range Requests`.
- **Zero Licenze Proprietarie:** Supporto nativo aperto per `.stl`, `.3mf` (incluse preview `Metadata/plate_*.png` di Bambu Studio / OrcaSlicer), `.obj`, `.ply`, `.glb`, `.zip`, più gestione a livelli senza licenze di `.lys`/`.lyt` (Lychee Slicer).

---

## Stack Tecnologico

| Layer | Tecnologia | Ruolo |
| :--- | :--- | :--- |
| **Desktop Shell & Backend** | **Tauri v2** + **Rust** | Accesso filesystem ad alte prestazioni, parsing mesh, hash `BLAKE3`, client Dropbox `HTTP Range`, renderer headless `wgpu`. |
| **Database & Ricerca** | **SQLite + `FTS5`** (`rusqlite` bundled) | Catalogo relazionale locale, ricerca full-text istantanea bilingue, persistenza override su spostamenti tramite hash. |
| **Frontend UI** | **React 19 + TypeScript + Vite** | Interfaccia reattiva, griglia virtualizzata (`@tanstack/react-virtual`) con budget `< 16ms` per frame. |
| **Viewer 3D On-Demand** | **Three.js** (`@react-three/fiber` + `drei`) | Viewport 3D interattivo su richiesta con illuminazione studio/matcap per dettagli scultorei. |

---

## Workflow di Sviluppo (WSL2 <-> GitHub <-> Windows)

1. **WSL2 (Linux):** Authoring specifiche, codice Rust/TypeScript e commit su `main`.
2. **GitHub:** Ponte di sincronizzazione (`DanieleSuppo/MeshCatalog`).
3. **Windows 10/11:** `git pull`, esecuzione locale (`cargo tauri dev`) e build pacchetto desktop Windows con accesso diretto a NTFS, Win32 API e GPU.

---

## Documentazione e Specifiche

- [AGENTS.md](./AGENTS.md) — Invarianti architetturali e regole operative per sviluppatori e agenti AI.
- [01 — Specifica di Prodotto](./docs/01-PRODUCT-SPEC.md) — Perimetro funzionale, formati supportati, UX Modelli vs Singoli File e roadmap v1/v2+.
- [02 — Architettura e Schema Dati](./docs/02-ARCHITECTURE.md) — Pipeline multi-passaggio, integrazione Dropbox Zero-Explosion, guardia Win32 e schema SQLite DDL.
- [03 — Motore Tassonomico Automatico](./docs/03-TAXONOMY-ENGINE.md) — Facce semantiche, punteggio multi-segnale, dizionario IT/EN e override inline.
- [04 — UI/UX e Contratti IPC](./docs/04-UI-UX-AND-IPC.md) — Layout, Smart Folder Card, Inspector, Viewer 3D on-demand e API Tauri IPC.
- [05 — Roadmap di Implementazione](./docs/05-ROADMAP.md) — Milestone operative da M0 a M5 e checklist cross-OS.
