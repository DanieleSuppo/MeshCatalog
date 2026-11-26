# Feature Specification: Core Local Indexer & Baseline Browser (`001-core-local-indexer`)

**Feature Branch**: `001-core-local-indexer`  
**Created**: 2026-11-26  
**Status**: Draft  
**Input**: Milestone M1 (`001-core-local-indexer`) — Gestione sorgenti locali in sola lettura, protezione anti-idratazione per placeholder cloud, scoperta trasparente di archivi `.zip` e formati multipli, ri-scansione incrementale resiliente agli spostamenti e shell desktop di navigazione e ricerca istantanea.

---

## Clarifications

### Session 2026-11-26

- Q: Come deve raggruppare i file in modelli/pack l'indicizzatore quando una cartella contiene file sciolti, sottocartelle tecniche di supporto (es. `STL/`, `Presupported/`, `Images/`) o un singolo file isolato? → A: Opzione A — ogni `.zip` è un `zip_archive`; una cartella con 2+ file 3D/slicer oppure 1 file 3D + immagini forma un `folder_pack`; 1 file 3D isolato senza immagini/compagni diventa `single_file`; sottocartelle con nomi tecnici noti (`STL`, `Presupported`, `Supported`, `Unsupported`, `Images`, `Renders`, `Lychee`, `Files`) e sottocartelle puramente tecniche di scala (es. `32mm`, `75mm`) prive di sotto-modelli distinti e senza ulteriori sottocartelle vengono accorpate al pack padre.
- Q: Quando l'utente seleziona una cartella nell'albero di navigazione, cosa deve mostrare la griglia principale sia durante l'esplorazione senza testo di ricerca sia quando viene digitata una query? → A: Opzione A raffinata — (1) le sottocartelle puramente tecniche assorbite in un `folder_pack` non appaiono come cartelle navigabili separate; (2) senza testo di ricerca, la vista di una cartella contenitore mostra in sezioni distinte sia le sottocartelle dirette non-tecniche (ciascuna con conteggio ricorsivo totale di modelli/file, numero di sottolivelli annidati e anteprima sintetica dei primi 3-4 titoli di modelli del sotto-albero) sia i modelli/file diretti del livello corrente; (3) con testo di ricerca attivo dentro una cartella selezionata, la ricerca filtra ricorsivamente l'intero sotto-albero mostrando su ogni risultato il percorso relativo.
- Q: Quando lo scanner locale incontra file segnaposto *Online-Only* (es. in una cartella Dropbox sincronizzata localmente in modalità Smart Sync), come devono essere rappresentati e mostrati nel catalogo di M1? → A: Opzione A — I segnaposto *Online-Only* vengono catalogati come record completi navigabili e ricercabili (`files` con stato segnaposto cloud e raggruppamento `models` basato sui soli nomi e percorsi, senza aprire file né espandere `.zip`), mostrati normalmente in griglia, albero e ricerca con badge distintivo `Cloud / Online-Only`.
- Q: Cosa deve succedere nella milestone M1 quando l'utente seleziona o apre una scheda **Modello / Pack** (es. un `folder_pack` con sottocartelle tecniche assorbite o un archivio `.zip` multi-file), considerando che il viewer 3D e le miniature grafiche arrivano in M2? → A: Opzione A — Selezionando/aprendo una scheda **Modello / Pack** si apre un pannello/inspector laterale leggero di sola lettura (solo testo/metadati, senza 3D né miniature grafiche) con percorso canonico, riepilogo formato/stato ed elenco completo dei file contenuti nel pack o nello `.zip` (percorso relativo interno, estensione, dimensione e stato).
- Q: Come deve comportarsi lo scanner locale e l'ispezione degli archivi `.zip` di fronte a cartelle/file di servizio di sistema (es. `$RECYCLE.BIN`, `.dropbox.cache`, `System Volume Information`), voci di metadati macOS dentro gli `.zip` (`__MACOSX/`, `._*.stl`) e collegamenti simbolici/junction NTFS? → A: Opzione A — Escludere automaticamente cartelle/file di sistema e cache (`$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`), ignorare le voci `__MACOSX/` e i prefissi AppleDouble `._*` sia su disco sia dentro gli `.zip`, e non attraversare collegamenti simbolici o junction NTFS di directory.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Gestione sorgenti locali in sola lettura e scansione sicura anti-idratazione (Priority: P1)

Un utente Windows con decine di gigabyte di modelli 3D su SSD locale (es. ~40 GB) e centinaia di gigabyte di file *Online-Only* sincronizzati tramite client cloud (es. ~660 GB in modalità *Smart Sync*) vuole registrare le proprie cartelle radice nell'applicazione e avviare la catalogazione avendo la certezza assoluta che l'applicazione non alteri alcun file utente e non forzi alcun download involontario dal cloud.

**Why this priority**: La sicurezza dei dati utente (zero modifiche su disco e zero idratazione accidentale di centinaia di GB cloud) è il prerequisito non negoziabile senza il quale l'applicazione non può essere eseguita in sicurezza su una collezione reale.

**Independent Test**: Registrare una cartella locale di test contenente sia file residenti sia segnaposto *Online-Only* (o file con attributi di file offline/richiamo su accesso), eseguire una scansione completa, quindi verificare tramite monitoraggio I/O e controllo integrità della cartella che: (a) nessun file o file nascosto/sidecar sia stato creato o modificato nelle cartelle utente, (b) i segnaposto cloud siano stati rilevati senza aprire flussi di lettura dati né scaricare alcun byte, e (c) tutto lo stato del catalogo risieda esclusivamente nella cartella dati dedicata dell'applicazione.

**Acceptance Scenarios**:

1. **Given** un utente al primo avvio dell'applicazione desktop, **When** registra una o più cartelle radice locali e le visualizza nell'elenco delle sorgenti, **Then** le sorgenti vengono salvate nella configurazione interna dell'applicazione e avviate alla scansione senza creare o modificare alcun file dentro le cartelle dell'utente.
2. **Given** una sorgente locale contenente un mix di file fisicamente residenti e segnaposto cloud *Online-Only*, **When** l'indicizzatore analizza l'albero delle cartelle, **Then** identifica i segnaposto esclusivamente tramite gli attributi di metadati del file system, li cataloga come record navigabili e ricercabili (stato segnaposto cloud, raggruppati nei rispettivi modelli/pack dai soli nomi e percorsi senza aprire il contenuto né espandere eventuali `.zip`), li mostra nell'interfaccia con badge distintivo `Cloud / Online-Only`, non apre mai il contenuto dei file né richiede anteprime di sistema e genera esattamente **0 byte** di traffico di download cloud.
3. **Given** una sorgente locale già indicizzata, **When** l'utente rimuove la sorgente dall'applicazione, **Then** i record della sorgente e dei relativi modelli/file scompaiono dal catalogo locale dell'applicazione mentre i file originali su disco restano completamente intatti.

---

### User Story 2 - Indicizzazione trasparente di cartelle, archivi `.zip` e formati 3D (Priority: P1)

L'utente vuole che l'applicazione scopra automaticamente tutte le cartelle, i pacchetti multi-file, gli archivi `.zip` compressi (senza scompattarli su disco) e tutti i formati di stampa 3D, slicer e immagini di copertina presenti nelle sorgenti locali, ricavando titoli leggibili anche da nomi di file o cartelle sporchi o tecnici.

**Why this priority**: Le collezioni di stampa 3D contengono tipicamente decine di migliaia di file divisi tra cartelle foglia e archivi `.zip` da gigabyte; senza visibilità interna sugli `.zip` e raggruppamento in modelli/pack, il catalogo sarebbe inutilizzabile.

**Independent Test**: Preparare una cartella locale contenente un pack multi-file (`.stl`, `.3mf`, `.lys`, `.lyt`, `.jpg`), un archivio `.zip` contenente sotto-cartelle con `.stl` e `.png`, e file singoli (`.obj`, `.ply`, `.glb`, `.step`, `.gcode`); avviare la scansione e verificare che il contenuto interno dello `.zip` venga indicizzato senza scrivere file estratti su disco, che i titoli visualizzati siano ripuliti dal rumore sintattico e che l'uso di spazio disco aggiuntivo nelle cartelle utente sia zero.

**Acceptance Scenarios**:

1. **Given** un archivio `.zip` residente su disco locale contenente file `.stl`, `.3mf` e immagini `.png`/`.jpg`, **When** l'indicizzatore scansiona la sorgente, **Then** legge l'indice interno dell'archivio senza estrarre file temporanei o permanenti su disco e registra sia il contenitore `.zip` sia ogni file supportato contenuto al suo interno.
2. **Given** una cartella foglia con più file correlati (es. `Dragon_Pack_v2/head_presupp.stl`, `body.stl`, `render_01.jpg`) e un file `.zip` con nome tecnico (es. `2025_03_Artisan_Guild_King_Thror_32mm.zip`), **When** il catalogo consolida i record, **Then** raggruppa logicamente gli elementi in modelli/pack distinti con titoli puliti e leggibili conservando il nome grezzo originale per riferimento e ricerca.
3. **Given** file nei formati `.stl`, `.3mf`, `.lys`, `.lyt`, `.obj`, `.ply`, `.glb`, `.step`, `.stp`, `.gcode` e immagini `.jpg`, `.jpeg`, `.png`, `.webp`, **When** vengono rilevati su disco o dentro uno `.zip`, **Then** vengono tutti catalogati con estensione, dimensione e percorso relativi accurati, ignorando file non pertinenti o di sistema non supportati, cartelle/file di sistema e cache (`$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`), voci di metadati macOS (`__MACOSX/` e prefissi `._*`) e collegamenti simbolici/junction NTFS di directory.

---

### User Story 3 - Ri-scansione incrementale rapida e resilienza a spostamenti e rinomine (Priority: P2)

Quando l'utente riorganizza le proprie cartelle su disco (spostando o rinominando modelli o `.zip` da Esplora File) oppure aggiunge pochi nuovi file a una collezione da ~40 GB, vuole poter rilanciare la scansione in pochi secondi saltando i file invariati e mantenendo l'identità di catalogo degli elementi spostati.

**Why this priority**: Una collezione reale evolve continuamente; ricalcolare tutto da zero ad ogni avvio renderebbe lente le ri-scansioni e spezzerebbe la continuità delle entità catalogate.

**Independent Test**: Dopo una prima scansione di un dataset di test, (a) rilanciare la scansione senza modifiche verificando che i file con dimensione e data di modifica invariate non vengano riletti per il calcolo dell'impronta, e (b) spostare e rinominare una cartella e un file `.zip` su disco, rilanciare la scansione e verificare che l'identità logica dei modelli spostati venga riconosciuta tramite impronta del contenuto aggiornando i percorsi e rimuovendo i vecchi riferimenti inesistenti.

**Acceptance Scenarios**:

1. **Given** una sorgente già indicizzata con file residenti invariati (stessa dimensione e stessa data di ultima modifica), **When** l'utente avvia una ri-scansione manuale, **Then** l'indicizzatore salta il ricalcolo dell'impronta crittografica del contenuto per i file invariati e processa solo file nuovi o modificati.
2. **Given** un modello o un archivio `.zip` già indicizzato con la propria impronta del contenuto, **When** l'utente lo sposta in un'altra sottocartella o lo rinomina su disco e avvia la ri-scansione, **Then** il sistema riconosce l'identità tramite impronta crittografica, aggiorna il percorso canonico al nuovo indirizzo e preserva l'identità del modello nel catalogo.
3. **Given** file o cartelle eliminati dal disco dall'utente tra una scansione e l'altra, **When** la ri-scansione si conclude, **Then** i relativi record vengono rimossi dal catalogo in modo pulito.

---

### User Story 4 - Navigazione gerarchica, switch Modelli/File e ricerca istantanea (Priority: P1)

L'utente vuole aprire l'applicazione e navigare istantaneamente la propria collezione anche mentre una scansione è in corso in background: esplorare l'albero delle cartelle, passare con un clic dalla vista raggruppata **Modelli / Pack** (predefinita) alla vista **Singoli File** e cercare per testo libero tra oltre 50.000 voci ricevendo risultati immediati su schede pulite basate su formato.

**Why this priority**: Senza la vista navigabile con toggle Modelli/File, albero cartelle e ricerca testuale sub-100 ms, il catalogo indicizzato non offre valore utilizzabile all'utente finale nella milestone M1.

**Independent Test**: Popolare il catalogo con oltre 50.000 voci (inclusi pack multi-file e contenuti `.zip`), avviare in parallelo una scansione simulata o incrementale e verificare che: (a) l'indicatore di avanzamento in background resti non bloccante, (b) la navigazione nell'albero cartelle e lo scorrimento della griglia restino fluidi, (c) il toggle `Modelli / Pack` $\leftrightarrow$ `Singoli File` aggiorni immediatamente la vista preservando cartella e testo cercato, e (d) le query di ricerca su titoli, parole del percorso, nomi di file interni ed estensioni restituiscano risultati in meno di 100 ms.

**Acceptance Scenarios**:

1. **Given** una scansione attiva in background su migliaia di file, **When** l'utente naviga l'interfaccia, **Then** visualizza un indicatore di stato non bloccante (avanzamento, elemento corrente, eventuali errori recuperabili) e può già sfogliare cartelle e cercare i record man mano che vengono indicizzati.
2. **Given** una cartella contenitore mista con modelli diretti (es. 5 modelli/pack o archivi `.zip`) e sottocartelle annidate non-tecniche (es. 6 sottocartelle, oltre a eventuali sottocartelle tecniche già assorbite nei rispettivi pack), **When** l'utente seleziona la cartella senza ricerca testuale nella vista predefinita **Modelli / Pack**, **Then** visualizza in modo distinto sia le schede delle sottocartelle dirette non-tecniche (ciascuna dotata di conteggio ricorsivo di modelli, file, sottolivelli annidati e anteprima dei primi 3-4 titoli rappresentativi del sotto-albero) sia le schede dei modelli/pack diretti di quel livello; **When** attiva il toggle **Singoli File**, **Then** la griglia passa istantaneamente a elencare i singoli file (inclusi quelli dentro gli `.zip`) mantenendo filtro cartella e testo di ricerca attivi.
3. **Given** un catalogo con almeno 50.000 voci indicizzate, **When** l'utente digita termini presenti nel titolo del modello, in una parola del percorso cartella, nel nome di un file contenuto in uno `.zip` o in un'estensione (es. `stl`, `3mf`, `lys`) — globalmente o con una cartella selezionata —, **Then** i risultati corrispondenti vengono filtrati ricorsivamente nell'intero sotto-albero della cartella selezionata e mostrati in meno di 100 millisecondi con percorso relativo contestuale attraverso schede segnaposto pulite e ordinate.
4. **Given** una scheda **Modello / Pack** (un `folder_pack` con eventuali sottocartelle tecniche assorbite oppure un archivio `.zip`), **When** l'utente la seleziona/apre nella griglia di M1, **Then** si apre un pannello inspector laterale testuale di sola lettura che espone i metadati del modello (titolo pulito, nome grezzo, percorso canonico, stato) e l'elenco completo dei file contenuti (con percorso relativo interno, estensione, dimensione e stato residente / `Cloud / Online-Only` / voce archivio).

---

### Edge Cases

- **Sorgente inaccessibile o rimossa fisicamente:** Se una cartella radice registrata è temporaneamente scollegata o non più esistente al momento della scansione, il sistema segnala un errore chiaro e recuperabile per quella sorgente senza bloccare le altre sorgenti né cancellare alla cieca il catalogo storico finché la sorgente è semplicemente irraggiungibile (vs rimozione esplicita da parte dell'utente o cancellazione interna a una radice accessibile).
- **Sorgenti sovrapposte o duplicate:** Se l'utente tenta di registrare una cartella già registrata o una sottocartella/genitore di una sorgente locale già censita, il sistema impedisce il doppio censimento con un messaggio chiaro spiegando la sovrapposizione.
- **Archivi `.zip` danneggiati, troncati o protetti da password:** Se uno `.zip` ha un indice interno corrotto o cifrato, il sistema registra un avviso recuperabile associato alla scansione, cataloga comunque il file `.zip` come voce singola di archivio senza contenuto espanso e prosegue la scansione dei restanti file senza interrompersi.
- **Archivi `.zip` annidati:** Se dentro uno `.zip` compare un altro file `.zip`, nella milestone M1 l'elemento annidato viene registrato come voce interna con estensione `.zip` senza eseguire decompressioni ricorsive su disco o in memoria.
- **File bloccati o permessi negati su disco:** Se un file residente è bloccato in esclusiva da un altro programma o privo di permessi di lettura, il sistema registra l'errore recuperabile su quel singolo elemento e continua l'indicizzazione degli altri file.
- **Mix di placeholder cloud e file residenti nella stessa cartella:** Se una cartella contiene sia `.stl` residenti sia `.zip` *Online-Only*, il sistema calcola l'impronta e legge i soli file residenti, mentre per il segnaposto *Online-Only* si astiene completamente dall'aprire il file, lo cataloga dai soli metadati di nome/percorso come segnaposto cloud non letto (senza espandere l'indice interno dell'eventuale `.zip`) e lo rende visibile in griglia e ricerca con badge `Cloud / Online-Only`.
- **Nomi di file con codifiche speciali o caratteri rumorosi:** Se file o cartelle contengono prefissi numerici, underscore multipli, estensioni ripetute o caratteri Unicode, la pulizia del titolo produce un'etichetta leggibile senza mai generare titoli vuoti (ricadendo eventualmente sul nome grezzo originale).
- **Cartelle di sistema, metadati `__MACOSX/` e collegamenti simbolici/junction NTFS:** Le cartelle e i file di servizio (`$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`), le voci macOS `__MACOSX/` e AppleDouble (`._*`, anche con estensione `.stl`/`.png`) su disco o negli `.zip` vengono esclusi dal catalogo; i collegamenti simbolici e le junction NTFS di directory non vengono attraversati durante la scansione per prevenire loop infiniti o uscite dalla radice autorizzata.

---

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (Gestione Sorgenti Locali):** Il sistema MUST consentire all'utente di registrare una o più cartelle radice locali sul filesystem Windows, visualizzare l'elenco delle sorgenti configurate (con stato, percorso e data ultima scansione), avviare manualmente la ri-scansione di una singola sorgente o di tutte le sorgenti e rimuovere una sorgente dal catalogo.
- **FR-002 (Garanzia Read-Only sui File Utente):** Il sistema MUST NOT modificare, spostare, rinominare, eliminare o creare alcun file, cartella o file nascosto/sidecar all'interno delle directory dei modelli dell'utente; ogni dato persistente del catalogo, impostazione e log operativo MUST risiedere esclusivamente nella cartella dati locale dedicata dell'applicazione.
- **FR-003 (Prevenzione Sovrapposizione Sorgenti):** Il sistema MUST impedire la registrazione duplicata della stessa cartella radice o di cartelle annidate in una sorgente locale già registrata, informando l'utente del conflitto.
- **FR-004 (Scudo Anti-Idratazione Placeholder Cloud):** Prima di aprire qualsiasi flusso di lettura dati o ispezionare contenuti su file locali, il sistema MUST verificare gli attributi di metadati del file system per rilevare i segnaposto cloud *Online-Only*; per qualsiasi file identificato come segnaposto *Online-Only*, il sistema MUST NOT aprire il file, leggere byte, calcolare impronte, ispezionare archivi né invocare servizi di anteprima del sistema operativo, garantendo **0 byte** di download cloud involontari, pur catalogando tali segnaposto come record navigabili e ricercabili (`CatalogFile` in stato segnaposto cloud e relativo `Model` derivato dai soli nomi/percorsi) esposti in UI con badge `Cloud / Online-Only`.
- **FR-005 (Scoperta Multi-Formato e Struttura Gerarchica):** Durante la scansione locale, il sistema MUST rilevare e catalogare la gerarchia delle cartelle e tutti i file supportati appartenenti alle categorie: modelli 3D e slicer (`.stl`, `.3mf`, `.lys`, `.lyt`, `.obj`, `.ply`, `.glb`, `.step`, `.stp`, `.gcode`), archivi (`.zip`) e immagini di copertina/creator (`.jpg`, `.jpeg`, `.png`, `.webp`), escludendo automaticamente directory/file di servizio e cache (`$RECYCLE.BIN`, `System Volume Information`, `.dropbox.cache`, `.git`, `Thumbs.db`, `.DS_Store`), voci macOS (`__MACOSX/`, prefissi `._*`) e senza attraversare collegamenti simbolici o junction NTFS di directory.
- **FR-006 (Ispezione Trasparente Archivi `.zip`):** Per gli archivi `.zip` fisicamente residenti su disco, il sistema MUST leggerne l'indice interno per catalogare ogni voce supportata contenuta al loro interno (con percorso interno relativo, nome file, estensione e dimensione) escludendo le voci di metadati macOS (`__MACOSX/` e file con prefisso `._*`), senza estrarre o scrivere alcun file decompresso su disco.
- **FR-007 (Raggruppamento Modelli/Pack e Pulizia Titoli):** Il sistema MUST raggruppare i file scoperti in unità logiche di modello/pack secondo le seguenti regole deterministiche: (a) ogni archivio `.zip` è un `zip_archive` auto-contenuto; (b) una cartella (purché non sia la cartella radice della sorgente priva di file diretti) con 2 o più file 3D/slicer diretti oppure 1 file 3D/slicer accompagnato da immagini di copertina forma un `folder_pack`, accorpando al pack padre anche eventuali sottocartelle terminali prive di ulteriori sottocartelle e prive di `.zip` il cui nome corrisponda a token tecnici noti (`STL`, `STLs`, `Presupported`, `Pre-supported`, `Supported`, `Unsupported`, `Images`, `Renders`, `Render`, `Lychee`, `Files`) o a pattern tecnici di scala (es. `32mm`, `75mm`, `1_10`); (c) un singolo file 3D/slicer isolato privo di altri file o immagini nella stessa cartella forma un `single_file`. Le immagini supportate non associate a pack o archivi MUST restare catalogate senza un modello sintetico (`model_id = NULL`), visibili e ricercabili in **Singoli File**, ma assenti da **Modelli / Pack**. Per ciascun modello/pack il sistema MUST generare un titolo di visualizzazione pulito derivato in modo deterministico dai nomi rumorosi di cartella/file conservando il nome grezzo originale.
- **FR-008 (Impronte Contenuto e Scansione Incrementale):** Per i file localmente residenti, il sistema MUST calcolare un'impronta crittografica del contenuto e registrare dimensione e data di ultima modifica; nelle ri-scansioni successive, MUST saltare la rilettura e il ricalcolo dell'impronta per i file la cui dimensione e data di ultima modifica risultino invariate.
- **FR-009 (Resilienza a Spostamenti, Rinomine e Rimozioni):** Durante la ri-scansione di una sorgente accessibile, il sistema MUST riconoscere tramite impronta del contenuto (e firma aggregata per i pack) i file, i modelli o gli archivi `.zip` spostati o rinominati dall'utente, preservando l'identità logica dell'entità nel catalogo e aggiornandone i percorsi, e MUST rimuovere dal catalogo i record dei file e modelli effettivamente eliminati dal disco.
- **FR-010 (Avanzamento Scansione Non Bloccante):** L'indicizzazione MUST avvenire in background con indicatori di avanzamento visibili (fase corrente, conteggio elementi processati, elemento in corso e segnalazione di eventuali errori recuperabili), consentendo all'utente di continuare a navigare e cercare i dati già indicizzati senza blocchi dell'interfaccia.
- **FR-011 (Navigazione Albero Cartelle, Anteprima Ricorsiva Sottocartelle e Toggle Modelli/File):** L'interfaccia MUST fornire la navigazione gerarchica ad albero delle sorgenti e delle sole cartelle non-tecniche (escludendo dall'albero navigabile le sottocartelle puramente tecniche assorbite nei `folder_pack` per FR-007) e un selettore istantaneo tra vista **Modelli / Pack** (predefinita) e vista **Singoli File**; il cambio di vista MUST mantenere attiva la cartella selezionata e la query di ricerca corrente. Durante l'esplorazione senza ricerca testuale di una cartella selezionata, la vista principale MUST mostrare in modo visivamente distinto sia le sottocartelle dirette non-tecniche — ciascuna con riepilogo ricorsivo del sotto-albero (totale modelli, totale file, conteggio sottolivelli annidati e anteprima sintetica dei primi 3–4 titoli di modelli rappresentativi sottostanti) — sia i modelli/file diretti appartenenti al livello corrente.
- **FR-012 (Ricerca Testuale Istantanea Ricorsiva):** L'utente MUST poter eseguire ricerche testuali libere su titoli puliti dei modelli, parole del percorso delle cartelle, nomi dei file (inclusi quelli interni agli `.zip`) ed estensioni dei file. Quando una sorgente o cartella è selezionata con testo di ricerca attivo, la ricerca MUST filtrare ricorsivamente tutti i modelli/file nell'intero sotto-albero della cartella selezionata, mostrando su ciascun risultato il percorso relativo per contestualizzare il sottolivello di provenienza.
- **FR-013 (Schede Segnaposto di Base M1):** Nella milestone M1, le viste elenco/griglia MUST visualizzare schede pulite basate sul formato e sul tipo di contenitore (titolo pulito, sottotitolo percorso/origine, badge formato/contenitore come `ZIP`, `3MF`, `STL`, `LYS` o indicatore distintivo `Cloud / Online-Only`, e riepilogo conteggio file/dimensione) mantenendo uno scorrimento fluido anche con decine di migliaia di voci.
- **FR-014 (Pannello Dettaglio Modello / Pack Testuale M1):** Selezionando una scheda **Modello / Pack**, l'interfaccia MUST aprire un pannello inspector laterale leggero di sola lettura (privo di viewport 3D e miniature grafiche in M1) che mostri titolo pulito, nome grezzo, percorso canonico, conteggi riepilogativi e l'elenco ordinato di tutti i `CatalogFile` contenuti (su disco, in sottocartelle tecniche assorbite o dentro un archivio `.zip`), indicando per ciascun file percorso relativo interno, estensione, dimensione e stato di archiviazione.

### Key Entities

- **Source (Sorgente):** Cartella radice locale registrata dall'utente (nome descrittivo, percorso canonico radice, stato di disponibilità, timestamp dell'ultima scansione).
- **Folder (Cartella):** Nodo navigabile nella gerarchia relativa di una sorgente, escluse le sottocartelle tecniche assorbite in un pack (percorso relativo normalizzato, cartella genitore, conteggio aggregato ricorsivo di modelli, file e sottocartelle annidate nel sotto-albero, oltre all'elenco sintetico dei primi titoli di modelli rappresentativi sottostanti).
- **Model (Modello / Pack):** Unità logica di presentazione nella vista predefinita, corrispondente a un pack di cartella foglia, a un archivio `.zip` o a un file modello singolo (titolo pulito, nome grezzo originale, tipo di contenitore, impronta aggregata stabile agli spostamenti, conteggio file e dimensione totale).
- **CatalogFile (File Catalogato):** Singolo file scoperto su disco o come voce interna di un archivio `.zip` (percorso relativo, nome file, estensione normalizzata, stato di archiviazione residente/segnaposto cloud/voce interna di archivio, dimensione in byte, data di modifica, impronta crittografica del contenuto per i file residenti, riferimento all'archivio padre se interno a uno `.zip`, riferimento al modello di appartenenza, nullo solo per immagini non associate).

### Scope Boundaries (Out of Scope for M1)

Le seguenti funzionalità sono esplicitamente escluse dalla milestone M1 e demandate alle milestone successive:
- Estrazione di immagini creator, preview `.3mf`/`.lys` e rendering headless di miniature 3D (Milestone M2).
- Visualizzatore 3D interattivo on-demand nell'inspector e modale QuickLook (Milestone M2; in M1 l'inspector espone esclusivamente metadati testuali e l'elenco dei file contenuti).
- Motore di tassonomia automatica a facce semantiche, espansione bilingue IT/EN, correzioni inline dei tag e Smart Folder mosaico *1 Hero + 3 Thumbnails* (Milestone M3).
- Connettore diretto al servizio cloud Dropbox, autenticazione account cloud e ispezione remota selettiva degli archivi (Milestone M4).
- Gestione/pulizia duplicati, integrazione con slicer esterni e classificazione visiva offline (Milestone M5 / v2+).

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (Zero Download Cloud Involontari):** Su una cartella di test Windows contenente segnaposto *Online-Only*, il **100%** dei segnaposto viene saltato dall'apertura dati e dall'estrazione anteprime di sistema, producendo **0 byte** scaricati dal cloud durante e dopo la scansione.
- **SC-002 (Integrità Read-Only):** Al termine di scansioni ripetute, aggiunta e rimozione sorgenti, il **100%** delle directory utente monitorate presenta **0** file creati, modificati, spostati o rinominati rispetto allo stato pre-scansione.
- **SC-003 (Velocità Indicizzazione SSD & `.zip`):** Una collezione locale residente di ~40 GB (con mix di cartelle e archivi `.zip`) viene indicizzata alla prima scansione completando scoperta iniziale, impronte e indici `.zip` in **meno di 60 secondi**, senza bloccare la navigazione UI.
- **SC-004 (Efficienza Ri-scansione Incrementale):** Una ri-scansione su collezione invariata completa l'ispezione saltando il **100%** dei file residenti non modificati e completandosi in **meno di 5 secondi** su ~40 GB locali.
- **SC-005 (Resilienza Spostamenti):** Il **100%** dei modelli e archivi `.zip` spostati o rinominati localmente tra due scansioni conserva la propria identità di modello nel catalogo senza generare voci fantasma duplicate.
- **SC-006 (Latenza Ricerca & Navigazione):** Su un catalogo popolato con **50.000+ voci**, le query di ricerca testuale restituiscono risultati aggiornati in **meno di 100 millisecondi** e il cambio vista **Modelli / Pack** $\leftrightarrow$ **Singoli File** avviene in modo istantaneo (< 100 ms) con scorrimento fluido della griglia.

---

## Assumptions

- **Ambiente target:** L'utente utilizza l'applicazione su desktop Windows 10/11; la logica pura di indicizzazione, gestione archivi, pulizia titoli, calcolo impronte e ricerca può essere verificata indipendentemente dal sistema operativo in ambiente di sviluppo, mentre il controllo degli attributi dei segnaposto cloud viene validato sull'ambiente Windows nativo.
- **Sorgenti M1 solo locali:** In M1 tutte le sorgenti registrate sono percorsi di cartelle locali del filesystem; gli archivi `.zip` non residenti marcati come *Online-Only* vengono trattati come segnaposto cloud e non aperti finché non sarà introdotto il connettore cloud in M4.
- **Archivi `.zip` non protetti:** Si assume che la grande maggioranza degli archivi `.zip` delle collezioni 3D sia standard e non cifrata; eventuali archivi protetti da password o danneggiati vengono gestiti con degrado controllato.
- **Raggruppamento euristico base:** Le regole strutturali di FR-007 (`.zip` come pack autonomi, cartelle multi-file o 1 file + immagini come `folder_pack` con accorpamento delle sottocartelle tecniche terminali di supporto/scala, file isolati come `single_file`) coprono la quasi totalità delle strutture reali in M1; dettagli fini di classificazione semantica per facce restano demandati a M3.
- **Persistenza singola locale:** L'applicazione è monoutente su macchina locale e conserva il catalogo nella propria directory dati utente dedicata senza richiedere servizi esterni o autenticazione in M1.
