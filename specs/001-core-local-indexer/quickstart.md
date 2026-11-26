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
   cargo test --locked --manifest-path src-tauri/Cargo.toml -p meshcatalog-core
   ```
   **Esiti attesi (tutti verdi):**
   - **Win32 Anti-Hydration Guard (mocked attribute bitmask):** file marcati `0x0040_0000` (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`) o `0x0000_1000` (`FILE_ATTRIBUTE_OFFLINE`) vengono classificati `cloud_placeholder` senza mai tentare aperture dati né decompressione `.zip`.
   - **Esclusioni & Symlink:** directory/file `$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`, `__MACOSX/`, `._*` (sia su disco sia dentro `.zip`) e symlink/junction di directory non vengono attraversati né catalogati, anche se il tipo Win32 non è riportato come symlink.
   - **Raggruppamento Modelli/Pack & Pulizia Titoli:**
     - `.zip` multi-file $\rightarrow$ `zip_archive` con `zip_entry` accurate e zero file estratti su disco.
     - `.zip` corrotto/cifrato $\rightarrow$ avviso recuperabile, archivio catalogato comunque senza bloccare la scansione.
     - Cartella con sottocartelle tecniche terminali (`STL/`, `Presupported/`, `32mm/`, `Images/`) $\rightarrow$ accorpata in un solo `folder_pack` padre; le sottocartelle tecniche non appaiono nell'albero navigabile.
     - File 3D isolato $\rightarrow$ `single_file`.
   - **Incrementale & Move Resilience:** ri-scansione su dataset invariato salta il 100% degli hash `BLAKE3`; spostamento/rinomina di `folder_pack` e `.zip` conserva `models.id` e aggiorna `canonical_uri` senza duplicati; cancellazione rimuove i record orfani; sorgente inaccessibile mantiene intatti i record storici.
   - **Benchmark SQLite FTS5 (`50.000+` record):** test automatizzato che inserisce 50k+ voci e verifica latenza di `search_catalog` (vista `models` e `files`, con e senza filtro ricorsivo `folder_id`) **< 100 ms**.

2. **Verifica Frontend TypeScript & Vite build:**
   ```bash
   npm ci
   npm run build
   npm test
   ```
   **Esito atteso:** Zero errori TypeScript (`tsc --noEmit`), bundle senza dipendenze WebGL/Three.js e test IPC/UI verdi.

### Evidenza WSL2 — T001–T046

Le voci T013, T021 e T032 hanno solo evidenza statica. Restano aperte in attesa della compilazione, dei test e della verifica IPC in T047.

| Task | Evidenza verificata in WSL2 |
| --- | --- |
| T001–T013 | Manifest, shell e registrazione IPC ispezionati; test di migrazione, WAL/FTS5, reader in sola lettura, DTO Rust/TS e build Vite verdi. |
| T014–T022 | Test con attributi Win32 simulati, esclusioni, guardia prima delle aperture, CRUD, annullamento Pass 1 e test sidebar/comandi verdi. Orchestrazione Tauri verificata staticamente. |
| T023–T029 | Test di grouping, immagini non associate, Central Directory ZIP (inclusi archivi corrotti e cifrati), FTS e conteggi cartelle verdi. |
| T030–T039 | Test SQLite sintetico da oltre 50.000 record sotto 100 ms; test UI con IPC simulato per ricerca, paginazione, navigazione e Inspector verdi. |
| T040–T043 | Test locali revisionati con ciclo rosso/verde: zero aperture per hash invariati, ri-hash al cambio di dimensione o mtime, ID pack/ZIP stabili, fingerprint ambigui/nulli, pruning e ritenzione dello storico. |
| T044–T046 | `cargo fmt --check`, `cargo check` core, `cargo clippy -p meshcatalog-core --all-targets -- -D warnings`, 29 test Rust con `--locked`, `npm ci`, `npm run build` e 9 test frontend superati. Audit statico I–V sotto. |

- **I — Scritture:** bootstrap e SQLite scrivono nella directory dati dell'app. `temp_store=MEMORY` su writer e reader evita file temporanei SQLite nel percorso temporaneo di sistema. Nessuna estrazione ZIP o scrittura nei percorsi utente.
- **II — Anti-idratazione:** hashing e Central Directory condividono un'unica apertura in sola lettura, successiva al controllo degli attributi di directory e file. Nessun accesso a dati Dropbox reali; i test WSL simulano gli attributi, non l'idratazione NTFS.
- **III — Browsing:** reader SQLite separati e griglia virtualizzata. Nessun pacchetto WebGL/Three.js nel manifest, albero npm, sorgenti o bundle controllati. I test browser sintetici non misurano il frame time WebView2.
- **IV — Tassonomia:** nessun configuratore; grouping e pulizia titoli deterministici. La riconciliazione conserva la riga modello, trasferisce i file di destinazione e riallinea entrambe le tabelle FTS5.
- **V — Cross-OS:** nessun nuovo parser o SDK proprietario. I nomi, i payload e il wiring IPC sono allineati staticamente al contratto. Il controllo opzionale `cargo check -p meshcatalog-app` su WSL2 è bloccato dalle librerie native mancanti `libsoup-3.0`, `javascriptcoregtk-4.1`, `webkit2gtk-4.1`; i gate WSL2 obbligatori riguardano core e frontend.

---

## 3. Validazione End-to-End su Windows 10/11 (`cargo tauri dev`)

Dopo `git pull` su Windows, usare `package-lock.json` e `src-tauri/Cargo.lock` per le stesse risoluzioni del gate WSL2. Prima degli scenari manuali, compilare `meshcatalog-app` ed eseguire i test di orchestrazione:

```powershell
npm ci
npm run build
cargo check --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc -p meshcatalog-app
cargo test --locked --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc -p meshcatalog-app
npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked
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

M1 non segue reparse point, nemmeno negli antenati della sorgente. Per un volume NTFS montato in una cartella usare la lettera di unità diretta, se disponibile. In Scenario A verificare che una sorgente attraverso un mount point venga rifiutata senza accesso ai dati. Se una sottocartella è Online-Only, la scansione termina con un avviso: il contenuto non viene enumerato e lo storico non viene eliminato.

### T047 — Aperto, non eseguito

Mancano una sessione Windows nativa di verifica e una fixture autorizzata (~40 GB SSD e segnaposto Online-Only). Non è stato aperto alcun dato Dropbox reale. Restano da provare:

- **Compilazione e test MSVC:** `cargo check --locked` e `cargo test --locked` per `meshcatalog-app`, inclusi i due test di `ScannerService` sulla concorrenza e sulla sorgente non disponibile.
- **Avvio WebView2:** build Tauri da lockfile, dialog nativo e IPC end-to-end.
- **Scenario A:** attributi Win32 effettivi, zero aperture dati e **0 byte** scaricati dai segnaposto; integrità read-only e scritture confinate a `%LOCALAPPDATA%\MeshCatalog`. Per la junction, usare una fixture NTFS locale separata e verificare che i file visibili solo dalla junction non entrino nel catalogo.
- **Scenario B:** prima scansione **< 60 s**, ricerca e cambio vista **< 100 ms**, scorrimento **< 16 ms** in WebView2 durante la scansione; navigazione e Inspector tramite IPC nativo.
- **Scenario C:** ri-scansione invariata **< 5 s**, ID invariati dopo spostamenti/rinomine e rimozione sorgente senza modificare la fixture.

I test WSL2 e lo smoke test browser con IPC simulato non chiudono nessuno di questi gate nativi.
