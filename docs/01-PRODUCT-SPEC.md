# 01 — Specifica di Prodotto (`MeshCatalog`)

## 1. Visione e Problema da Risolvere

Chi stampa in 3D accumula nel tempo centinaia di gigabyte di modelli scaricati da creator, Patreon, Tribes, MakerWorld, Printables o Cults3D. Le cartelle diventano labirinti con nomi di file spesso generici o tecnici (`arm_L_supp.stl`, `plate_1.3mf`, `pack_03.zip`), mentre Esplora Risorse di Windows non offre anteprime intelligenti, non guarda dentro gli `.zip`, fatica con decine di migliaia di file e rischia di forzare il download di file *Online-Only* dal cloud.

**MeshCatalog** risolve questo caos offrendo un catalogo desktop **100% locale, gratuito e senza account/cloud proprietari**:
- Indicizza in profondità (anche sacrificando tempo in background) per garantire una **navigazione e ricerca istantanee**.
- Mostra **cosa contiene una cartella o uno `.zip` prima ancora di entrarci**, tramite mosaici intelligenti.
- Classifica automaticamente l'intera libreria in categorie semantiche comprensibili in italiano e inglese (*"pupazzetti fantasy"*, *"scenari di fantascienza"*, *"resina"*, *"FDM"*).

---

## 2. Profilo della Collezione Target

- **Volume totale stimato:** ~700 GB.
  - **~40 GB su SSD interno** (Windows NTFS, file fisicamente residenti).
  - **~660 GB su Dropbox** (cartella sincronizzata in modalità *Smart Sync / Online-Only* + archivi cloud).
- **Tipologia contenuti:** Mix eterogeneo tra miniature/statue multi-parte e scenari (resina/FDM), oggetti funzionali e progetti multi-piastra; ampia presenza di file ancora compressi in `.zip`. Pochi file `.step` e `.gcode`.
- **Requisito critico:** Indicizzare e navigare tutti i 700 GB **senza saturare il disco locale** scaricando involontariamente i 660 GB da Dropbox.

---

## 3. Matrice dei Formati Supportati (Zero Licenze Proprietarie)

Tutti i formati vengono gestiti con librerie open-source o standard aperti royalty-free:

| Formato | Tipo | Supporto in v1 | Strategia Anteprima & Metadati |
| :--- | :--- | :--- | :--- |
| **`.stl`** (binario & ASCII) | Aperto (de-facto) | **Pieno** | Parsing veloce vertici/triangoli, calcolo bounding box (mm) e volume stimato; rendering headless multi-angolo `.webp`; vista 3D interattiva on-demand. |
| **`.3mf`** (Standard / Bambu Studio / OrcaSlicer) | Standard Aperto (ZIP + XML/JSON) | **Pieno** | Lettura archivio ZIP interno: estrazione istantanea di `Metadata/plate_*.png` (e varianti `_small`/`_no_light`), elenco piastre, oggetti e metadati slicer; parsing mesh per vista 3D. |
| **`.obj`, `.ply`, `.glb`** | Aperti | **Pieno** | Supporto parsing mesh, metriche geometriche, thumbnail headless e viewer 3D on-demand. |
| **`.lys` / `.lyt`** (Lychee Slicer) | Proprietario (Mango3D) | **Tier 1 (v1)** | **`.lys`:** ispezione del contenitore per estrarre eventuale `preview.png` incorporata e metadati di scena (senza dipendenze proprietarie).<br>**`.lyt`:** estrazione dei path di riferimento verso gli `.stl` sorgente collegati.<br>*Tier 2 (opzionale futuro):* estrazione mesh tramite specifiche reverse-engineered open-source della community con fallback sicuro. |
| **`.zip`** (Archivi) | Standard Aperto | **Pieno (Prima Classe)** | Indicizzazione trasparente del contenuto senza estrazione su disco (sia su SSD locale che su Dropbox via `HTTP Range`). |
| **`.jpg`, `.jpeg`, `.png`, `.webp`** | Aperti | **Pieno (Copertine)** | Riconoscimento automatico dei render dei creator all'interno di cartelle o archivi `.zip` per usarli come copertina prioritaria (Hero). |
| **`.step`, `.stp`, `.gcode`** | Misti | **Catalogazione base** | Indicizzati come file presenti nel modello/cartella con estensione e metadati testuali (es. tempo/materiale da header `.gcode`), senza parsing CAD completo in v1. |

---

## 4. Modelli Concettuali ed Esperienza d'Uso

### 4.1 Toggle Vista Principale: `Modelli / Pack` (Default) vs `Singoli File`
Poiché un singolo modello 3D è spesso composto da 10-40 file (es. `Drago_Rosso/ala_dx.stl`, `coda.stl`, `testa_presupp.stl` oppure un intero `.zip`):
- **Vista Modelli / Pack (Default):** Raggruppa semanticamente i file appartenenti allo stesso modello/cartella foglia o archivio `.zip` in **un'unica scheda**. Cercando *"drago"*, appare un solo risultato *"Drago Rosso (22 file)"*; aprendolo nell'Inspector si accede ai singoli pezzi.
- **Vista Singoli File:** Switch istantaneo in toolbar per elencare ogni singolo `.stl` / `.3mf` / `.lys` individualmente (utile quando si cerca una parte specifica, es. `basetta 50mm`).

### 4.2 Navigazione Cartelle Intelligente ("Vedere dentro prima di entrare")
- Ogni cartella mostra una **Smart Folder Card** con un mosaico *1 Hero grande + 3 miniature secondarie* scelte in base a criteri di significatività (render creator -> `.3mf` -> `.lys` -> `.stl` più complesso geometricamente, evitando viti/perni/basette minori).
- Passando il mouse orizzontalmente sulla card (*Hover Scrub*), è possibile sfogliare rapidamente i modelli contenuti nella cartella senza aprirla.
- Badge riassuntivi immediati mostrano la composizione (`14 Modelli · 62 STL · 3 ZIP · Resina`).

### 4.3 Ricerca e Tassonomia Automatica
- Zero attività di tagging manuale richiesta all'utente.
- Ricerca istantanea combinata **testo libero + filtri a facce** con supporto bilingue **Italiano <-> Inglese** (es. cercando *"pupazzetti fantasy"* trova anche cartelle e file chiamati `Elven_Ranger_32mm.stl` dentro alberature pertinenti).
- L'unica azione manuale prevista sulla scheda è la **correzione inline di eventuali tag errati** (aggiunta/rimozione rapida sul singolo modello), memorizzata come override persistente.

### 4.4 Resilienza a Spostamenti e Rinomine
- Ogni file locale viene identificato tramite hash crittografico ultraveloce **`BLAKE3`**; ogni file su Dropbox tramite il `content_hash` fornito nativamente dalle API Dropbox.
- Se l'utente sposta o rinomina una cartella o un archivio da Esplora Risorse di Windows, al passaggio successivo l'indicizzatore riconosce i file dagli hash, **preservando le miniature già renderizzate e gli eventuali override dei tag**.

---

## 5. Fuori Perimetro v1 (Previsto in Roadmap v2+)

Per mantenere la v1 snella, solida e veloce da consegnare, i seguenti moduli sono esplicitamente rimandati alle versioni successive:
1. **Gestione e pulizia duplicati** (individuazione doppioni esatti `BLAKE3` / `ZIP` vs estratto e spostamento nel Cestino di Windows).
2. **Integrazione Slicer esterni** (azioni contestuali *"Apri in Bambu Studio"*, *"Apri in Lychee Slicer"*).
3. **Classificazione Visiva Locale Offline (AI Vision/CLIP)** per inferire categorie guardando le miniature renderizzate quando i nomi dei file/cartelle sono completamente muti.
4. **Tagging manuale esteso** (collezioni personalizzate, stati *Da stampare / Stampato*, note).
5. **Configuratore UI di regole/filtri:** Escluso permanentemente per scelta progettuale; nuove categorie e regole di classificazione vengono aggiunte direttamente nel codice ad ogni versione.
