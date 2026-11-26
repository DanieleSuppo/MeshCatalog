# Quickstart & Validation Guide: Core Local Indexer (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26  
**Plan**: [plan.md](./plan.md) | **Contracts**: [contracts/tauri-ipc.md](./contracts/tauri-ipc.md)

Questa guida descrive i prerequisiti e gli scenari verificabili end-to-end per validare la Milestone M1 tra l'ambiente di sviluppo **WSL2 (Linux)** e l'ambiente target nativo **Windows 10/11**.

---

## 1. Prerequisiti

### Su WSL2 (Authoring, Logic Unit & Integration Tests)
- **Rust stable** (`rustup`, `cargo`, `rustc`) — *nota: installare `rustup` in WSL2 se non ancora presente nel `PATH`.*
- **Node.js >= 18 LTS** (`v18.15.0+` / `npm 9+`) per type-check e build frontend (`npm run build`).
- Nessuna libreria grafica Linux o WebView2 richiesta per testare `meshcatalog-core`.

### Su Windows 10/11 (Native Win32 Guard, Tauri Shell & 40 GB / Smart Sync Benchmark)
- **Rust toolchain (`x86_64-pc-windows-msvc`)** + Visual Studio C++ Build Tools.
- **Node.js LTS** + **Edge WebView2 Runtime**.
- **Tauri CLI v2** (`npm run tauri dev` / `cargo tauri dev`).

---

## 2. Validazione in WSL2 (Pure-Rust Core + Frontend Type-Check)

1. **Verifica compilazione e test `meshcatalog-core`:**
   ```bash
   cargo test --manifest-path src-tauri/Cargo.toml -p meshcatalog-core
   ```
   **Esiti attesi (tutti verdi):**
   - **Win32 Anti-Hydration Guard (mocked attribute bitmask):** file marcati `0x0040_0000` (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`) o `0x0000_1000` (`FILE_ATTRIBUTE_OFFLINE`) vengono classificati `cloud_placeholder` senza mai tentare aperture dati né decompressione `.zip`.
   - **Esclusioni & Symlink:** directory/file `$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`, `__MACOSX/`, `._*` (sia su disco sia dentro `.zip`) e symlink di directory non vengono attraversati né catalogati.
   - **Raggruppamento Modelli/Pack & Pulizia Titoli:**
     - `.zip` multi-file $\rightarrow$ `zip_archive` con `zip_entry` accurate e zero file estratti su disco.
     - `.zip` corrotto/cifrato $\rightarrow$ avviso recuperabile, archivio catalogato comunque senza bloccare la scansione.
     - Cartella con sottocartelle tecniche terminali (`STL/`, `Presupported/`, `32mm/`, `Images/`) $\rightarrow$ accorpata in un solo `folder_pack` padre; le sottocartelle tecniche non appaiono nell'albero navigabile.
     - File 3D isolato $\rightarrow$ `single_file`.
   - **Incrementale & Move Resilience:** ri-scansione su dataset invariato salta il 100% degli hash `BLAKE3`; spostamento/rinomina di `folder_pack` e `.zip` conserva `models.id` e aggiorna `canonical_uri` senza duplicati; cancellazione rimuove i record orfani; sorgente inaccessibile mantiene intatti i record storici.
   - **Benchmark SQLite FTS5 (`50.000+` record):** test automatizzato che inserisce 50k+ voci e verifica latenza di `search_catalog` (vista `models` e `files`, con e senza filtro ricorsivo `folder_id`) **< 100 ms**.

2. **Verifica Frontend TypeScript & Vite build:**
   ```bash
   npm install
   npm run build
   ```
   **Esito atteso:** Zero errori TypeScript (`tsc --noEmit`) e bundle Vite generato senza dipendenze WebGL/Three.js.

---

## 3. Validazione End-to-End su Windows 10/11 (`cargo tauri dev`)

Dopo `git pull` su Windows:

```powershell
npm install
npm run tauri dev
```

### Scenario A — Integrità Read-Only & Zero-Hydration su Smart Sync (SC-001, SC-002)
1. Creare una directory temporanea di test e una cartella Dropbox reale (o cartella con file placeholder `Online-Only` verificati via `attrib` / proprietà Explorer `"Disponibile solo online"`).
2. Registrare la sorgente tramite `+ Aggiungi cartella` nell'app.
3. Monitorare I/O (es. Process Monitor filtrando sul processo `MeshCatalog` per `CreateFile` con `GENERIC_READ` dati su placeholder, oppure monitoraggio banda Dropbox).
4. **Esito atteso:**
   - **0 byte** scaricati dal cloud; nessuna icona Dropbox passa da "Nuvola" a "Verde/Sincronizzato".
   - Tutti i file `Online-Only` compaiono in catalogo con badge `Cloud / Online-Only` e sono ricercabili.
   - Nessun file o sidecar creato dentro le directory utente; `catalog.db` e `catalog.db-wal` esistono solo in `%LOCALAPPDATA%\MeshCatalog\`.

### Scenario B — Navigazione Cartelle, Ricerca < 100 ms & Inspector M1 (SC-006)
1. Registrare la collezione locale reale (~40 GB SSD con mix di cartelle pack e `.zip`) e osservare l'avanzamento non bloccante (`Passata 1/2` $\rightarrow$ `Passata 2/2`).
2. Durante la scansione, navigare l'albero a sinistra e verificare la vista mista cartelle+modelli (ogni sottocartella mostra conteggio ricorsivo modelli/file, sottolivelli annidati e 3–4 titoli campione; nessuna sottocartella tecnica `STL/` o `Presupported/` appare come nodo separato).
3. Digitare un termine presente dentro uno `.zip` (es. `head`, `3mf`, `lys`) e verificare risultati ricorsivi immediati con path relativo contestuale.
4. Attivare il toggle `Modelli / Pack` $\leftrightarrow$ `Singoli File` verificando conservazione di cartella e testo cercato.
5. Cliccare una scheda `folder_pack` o `zip_archive`: verificare l'apertura dell'Inspector laterale testuale con percorso canonico ed elenco completo dei file interni senza alcun canvas 3D o decodifica immagini.

### Scenario C — Ri-scansione Incrementale & Spostamento (SC-004, SC-005)
1. Rilanciare `Scansiona` senza modifiche sui ~40 GB locali: verificare completamento in **< 5 secondi**.
2. Spostare/rinominare una cartella pack e un archivio `.zip` in Esplora File di Windows, quindi rilanciare `Scansiona`: verificare l'aggiornamento del percorso canonico senza voci duplicate nel catalogo.
3. Rimuovere la sorgente dalla UI e verificare che i record spariscano dal DB mentre i file reali su SSD restano intatti.
