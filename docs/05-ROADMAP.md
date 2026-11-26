# 05 — Roadmap di Implementazione (`MeshCatalog`)

## Panoramica delle Milestone

| Fase | Obiettivo Principale | Stato |
| :--- | :--- | :---: |
| **M0** | Repository GitHub, `AGENTS.md` e Specifiche complete | **In corso / Completata** |
| **M1** | Scheletro Tauri v2 + SQLite `FTS5` + Scanner Locale SSD & `.zip` | Prossima |
| **M2** | Thumbnailer Headless `wgpu` + Estrazione `.3mf`/`.lys` + Viewer 3D On-Demand | Pianificata |
| **M3** | Motore Tassonomico Automatico + Smart Folder Preview + UI Completa | Pianificata |
| **M4** | Connettore Dropbox "Zero-Explosion" (`OAuth PKCE` + `HTTP Range` ZIP) | Pianificata |
| **M5 (v2+)** | Duplicati, "Apri in Bambu Studio / Lychee", AI Vision Locale (CLIP/ONNX) | Futura |

---

## Dettaglio Milestone

### M1 — Core Engine, SQLite & Scanner Locale SSD + ZIP
- Inizializzazione progetto Tauri v2 (`src-tauri` Rust + `src` React/TypeScript/Vite).
- Migrazioni SQLite con schema completo (`sources`, `folders`, `models`, `files`, `models_fts`).
- Implementazione scanner NTFS locale con **Win32 Anti-Hydration Guard** (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `OFFLINE`).
- Supporto lettura Central Directory `.zip` locali senza estrazione su disco + calcolo hash `BLAKE3` + tracciamento spostamenti.
- UI scheletro con barra di ricerca `FTS5` e griglia virtualizzata.
- **Criterio di uscita:** Indicizzazione dei 40 GB locali su Windows in pochi secondi con 0 download involontari dei placeholder Dropbox.

### M2 — Pipeline Grafica: Estrazione Anteprime, `wgpu` Headless & Viewer 3D
- Estrattore integrato per immagini creator (`.jpg`/`.png`/`.webp`), `Metadata/plate_*.png` dai `.3mf`, `preview.png` dai `.lys`.
- Renderer headless `wgpu` su Rust per generare sprite `.webp` a 4 angolazioni da `.stl`/`.obj`/`.3mf` con calcolo automatico bounding box e triangoli.
- Cache locale `.webp` servita via protocollo Tauri.
- Inspector laterale + QuickLook (`Spazio`) con viewer Three.js on-demand e matcap studio.
- **Criterio di uscita:** Scorrimento fluido `< 16ms` di migliaia di card con anteprime pronte e hover scrub 4-angoli funzionante.

### M3 — Motore Tassonomico Automatico & Smart Folder UX
- Implementazione Motore Tassonomico multi-segnale + dizionario semantico versionato + espansione query bilingue IT/EN (*"pupazzetti fantasy"*, *"scenari fantascienza"*, *"resina"* vs *"FDM"*).
- Toggle **Modelli / Pack (Default)** $\leftrightarrow$ **Singoli File**.
- Smart Folder Card (*1 Hero + 3 Thumbnails* + *Hover Scrub* cartella + badge riepilogativi).
- Correzione inline dei tag (`user_override`) con persistenza su `fingerprint_overrides`.
- **Criterio di uscita:** Le query naturali in italiano restituiscono immediatamente i modelli e le cartelle attese senza alcuna etichettatura manuale.

### M4 — Connettore Dropbox "Zero-Explosion" (~660 GB)
- Flusso OAuth 2.0 PKCE desktop e sync incrementale via `/files/list_folder` + cursor.
- Ispezione remota `.zip` e `.3mf` tramite `HTTP Range` (lettura EOCD/Central Directory + estrazione chirurgica delle sole immagini di copertina in RAM).
- Integrazione interruttore utente `render_cloud_stl_in_ram` per i soli-STL su Dropbox privi di foto.
- Unificazione trasparente nella UI tra sorgente locale SSD e sorgente Dropbox con badge chiari.
- **Criterio di uscita:** Tutti i ~700 GB (SSD + Dropbox) navigabili con anteprime, contenuti `.zip` visibili e zero byte scaricati nella cartella Dropbox locale.

### M5 (v2+) — Evoluzioni Pianificate
- **Gestione Duplicati:** Rilevazione doppioni esatti `BLAKE3` / `content_hash` e `ZIP` vs cartella estratta, con spostamento sicuro nel Cestino di Windows.
- **Integrazione Slicer:** Azioni rapide *"Apri in Bambu Studio"*, *"Apri in Lychee Slicer"* (con download on-demand mirato se il modello è su Cloud).
- **AI Vision Locale (ONNX / CLIP):** Classificazione automatica delle miniature `.webp` per pack privi di nomi significativi.
- **Lychee Tier 2:** Estrazione sperimentale mesh 3D da `.lys` tramite specifiche open-source della community.

---

## Checklist Ambiente Sviluppo (WSL2 <-> Windows)

1. **WSL2 (Authoring & Git):**
   - Lavoro su branch/commit, documentazione e logica condivisa in `/home/dsupp/3DManager/git`.
   - `cargo check` / `cargo test` per i moduli pure-Rust platform-independent.
2. **Windows 10/11 (Build & Test Nativi):**
   - Prerequisiti: Rust (`msvc` toolchain), Node.js LTS, WebView2 Runtime, Visual Studio C++ Build Tools.
   - `git pull` -> `npm install` -> `cargo tauri dev` per verificare Win32 file attributes, GPU `wgpu` e apertura reale su cartella SSD + Dropbox.
