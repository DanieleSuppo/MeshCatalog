# Quickstart & Validation Guide: Core Local Indexer (`001-core-local-indexer`)

**Branch**: `001-core-local-indexer` | **Date**: 2026-11-26  
**Plan**: [plan.md](./plan.md) | **Contracts**: [contracts/tauri-ipc.md](./contracts/tauri-ipc.md)

Questa guida descrive i prerequisiti e gli scenari verificabili end-to-end per validare la Milestone M1 tra l'ambiente di sviluppo **WSL2 (Linux)** e l'ambiente target nativo **Windows 10/11**.

---

## 1. Prerequisiti

### Su WSL2 (Authoring, Logic Unit & Integration Tests)
- **Rust stable** (`rustup`, `cargo`, `rustc`) — *nota: installare `rustup` in WSL2 se non ancora presente nel `PATH`.*
- **Node.js 22 LTS** (`>=22.12.0` / `npm 10+`, verificato `v22.23.2` / `npm 10.9.8` in WSL2) per type-check e build frontend (`npm run build`).
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
   - **Win32 Anti-Hydration Guard (attributi e tag simulati):** i bit recall/offline impediscono aperture dati o enumerazione. I tag della famiglia `CLOUD` senza recall/offline autorizzano l'accesso residente; tag sconosciuti, name surrogate ed errori di lookup lo bloccano.
   - **Esclusioni & Symlink:** directory/file `$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`, `__MACOSX/`, `._*` (sia su disco sia dentro `.zip`) e symlink/junction di directory non vengono attraversati né catalogati, anche se il tipo Win32 non è riportato come symlink.
   - **Raggruppamento Modelli/Pack & Pulizia Titoli:**
     - `.zip` multi-file $\rightarrow$ `zip_archive` con `zip_entry` accurate e zero file estratti su disco.
     - `.zip` corrotto/cifrato $\rightarrow$ archivio catalogato senza espansione; avviso recuperabile conservato anche con altre diagnosi e `last_scan_at` aggiornato.
     - Cartella con sottocartelle tecniche terminali (`STL/`, `Presupported/`, `32mm/`, `Images/`) $\rightarrow$ accorpata in un solo `folder_pack` padre; le sottocartelle tecniche non appaiono nell'albero navigabile.
     - File 3D isolato $\rightarrow$ `single_file`.
   - **Incrementale & Move Resilience:** ri-scansione su dataset invariato salta il 100% degli hash `BLAKE3`; spostamento/rinomina di `folder_pack` e `.zip` conserva `models.id` e aggiorna `canonical_uri` senza duplicati; una scansione completa rimuove gli orfani. Con un sottoalbero opaco o inaccessibile, la rinomina certa di un modello osservato conserva l'ID senza rimuovere lo storico non osservato.
   - **Benchmark SQLite FTS5 (`50.000+` record):** test automatizzato che inserisce 50k+ voci e verifica latenza di `search_catalog` (vista `models` e `files`, con e senza filtro ricorsivo `folder_id`) **< 100 ms**.

2. **Verifica Frontend TypeScript & Vite build:**
   ```bash
   npm ci
   npm run build
   npm test
   ```
   **Esito atteso:** Zero errori TypeScript (`tsc --noEmit`), bundle senza dipendenze WebGL/Three.js e test IPC/UI verdi.

### Evidenza WSL2 — T001–T046

I gate Windows `T013`, `T021`, `T032` e `T047` sono stati completati e verificati su Windows 10/11 (`x86_64-pc-windows-msvc`) come riportato nella sezione dedicata sotto.

| Task | Evidenza verificata in WSL2 |
| --- | --- |
| T001–T013 | Manifest, shell e registrazione IPC ispezionati; test di migrazione, WAL/FTS5, reader in sola lettura, DTO Rust/TS e build Vite verdi. |
| T014–T022 | Test core con attributi Win32 simulati, esclusioni, guardia prima delle aperture, CRUD e annullamento Pass 1. Tre test `ScannerService` eseguiti nella shell Tauri Linux, incluso rimozione/re-add di una sorgente accodata con la prima scansione bloccata. |
| T023–T029 | Test di grouping e visibilità a metà Pass 1 (file ricercabili, nodi tecnici provvisori nascosti), immagini non associate, ZIP corrotti/cifrati, FTS e conteggi cartelle. |
| T030–T039 | Ricerca SQLite sintetica su 50.010 righe per vista: massimo 38,96 ms (prefissi, entrambe le viste, scope globale e ricorsivo); test UI con IPC simulato per ricerca, paginazione, navigazione e Inspector. |
| T040–T043 | Test locali rosso/verde: zero riletture dei file invariati, ownership/titoli FTS aggiornati dopo rinomina, ID pack/ZIP stabili, fingerprint ambigui/nulli, pruning e ritenzione dello storico. Ri-scansione sintetica di 2.002 file: riscritture FTS 8.026 → 0; 280 ms → 112 ms in due run WSL2, non benchmark SSD da 40 GB. |
| T044–T046 | `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`; 45 test core e 3 test shell Linux con `--locked`. `npm run build` e 10 test frontend verdi. Audit I–V sotto. |

- **I — Scritture:** bootstrap e SQLite scrivono nella directory dati dell'app. `temp_store=MEMORY` su writer e reader evita file temporanei SQLite nel percorso temporaneo di sistema. Nessuna estrazione ZIP o scrittura nei percorsi utente.
- **II — Anti-idratazione:** hashing e Central Directory condividono un'unica apertura in sola lettura, successiva al controllo degli attributi di directory e file. Nessun accesso a dati Dropbox reali; i test WSL simulano gli attributi, non l'idratazione NTFS.
- **III — Browsing:** reader SQLite separati e griglia virtualizzata. Nessun pacchetto WebGL/Three.js nel manifest, albero npm, sorgenti o bundle controllati. I test browser sintetici non misurano il frame time WebView2.
- **IV — Tassonomia:** nessun configuratore; grouping e pulizia titoli deterministici. La riconciliazione conserva la riga modello, trasferisce i file di destinazione e riallinea entrambe le tabelle FTS5.
- **V — Cross-OS:** nessun nuovo parser o SDK proprietario. I nomi, i payload e il wiring IPC sono allineati staticamente al contratto. La shell Linux (`cargo check` e `cargo test -p meshcatalog-app`) è stata verificata usando un sysroot locale con GTK/WebKitGTK. Questo non prova la build MSVC, WebView2, il dialog o l'IPC end-to-end.

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

M1 permette solo reparse point riconosciuti della famiglia `CLOUD` e residenti, anche negli antenati. Per un volume NTFS montato in una cartella usare la lettera di unità diretta, se disponibile. In Scenario A verificare che una sorgente attraverso un mount point venga rifiutata senza accesso ai dati. Verificare inoltre che una sottocartella Online-Only non venga enumerata e che il suo storico rimanga intatto.

### T047 — Completato su Windows 10/11 (`x86_64-pc-windows-msvc`)

Verifica nativa completata su Windows (`rustc 1.100.0`, `Node v26.13.0`, MSVC + WebView2) sul branch `001-core-local-indexer` (incluso fix `vite.config.ts` `ebd0dc7` per escludere `src-tauri/target` dal watcher dev su Windows):

- **Compilazione e test MSVC:** `npm ci` + `npm run build` (`463 ms`) e `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --target x86_64-pc-windows-msvc` superati al **100%** (inclusi i 3 test `ScannerService` in `meshcatalog-app` e il test nativo junction NTFS `native_junction_is_rejected_as_root_or_ancestor_and_not_walked`).
- **Avvio WebView2 & IPC nativo:** `npm run tauri -- dev --target x86_64-pc-windows-msvc -- --locked` avviato regolarmente su `MeshCatalog`, con selezione cartella nativa, albero navigabile, griglia virtualizzata e Inspector testuale verificati end-to-end su collezione reale da **60.330 file / 4.220 modelli** (mix SSD locale + segnaposto Dropbox *Online-Only*).
- **Scenario A (Zero-Hydration & Read-Only):** **0 byte** scaricati dai segnaposto *Online-Only*, badge `Cloud / Online-Only` correttamente esposti su cartelle e modelli remoti, e integrità read-only confermata nelle directory utente.
- **Scenario B & C (Latenza Query, Ri-scansione & Spostamenti):** latenza di `search_catalog` in WebView2 misurata a **`~1,2 ms`** (`< 100 ms`), navigazione gerarchica con sottocartelle ripulite (`Dungeon Classics` consolidato da `143` voci frammentate a `97` pack puliti senza `A, A, B, B` né duplicati nelle anteprime), e ri-scansione completa su 60.330 file (inclusi lookup Win32 sui segnaposto cloud e riassorbimento multi-livello) completata in **`~8 s`** saltando il 100% degli hash residenti invariati (spostamenti/rinomine e rimozione sorgente validati dalla suite automatica MSVC `zip_and_incremental_tests` / `anti_hydration_tests`).
