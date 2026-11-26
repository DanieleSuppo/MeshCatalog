# 03 — Motore Tassonomico Automatico (`MeshCatalog`)

## 1. Principio Guida

L'utente **non tagga manualmente la collezione** e **non gestisce configuratori di regole nella UI**.
Tutta l'intelligenza semantica è codificata in un **Motore Tassonomico deterministico a punteggio multi-segnale**, versionato direttamente nel repository (dizionari TOML/YAML compilati o caricati al boot dell'app), così da poter aggiungere nuovi filtri o nuove categorie ad ogni nuova release semplicemente aggiornando le regole.

---

## 2. Facce Tassonomiche Ortogonali

Un modello può avere simultaneamente più attributi combinabili (es. *Miniatura + Fantasy + Resina + Pre-supportata + 32mm*):

| Facet (`facet`) | Valori (`slug`) principali in v1 | Esempi di utilizzo nella ricerca |
| :--- | :--- | :--- |
| **`print_tech`** | `resin`, `fdm`, `unknown` | Filtrare modelli pensati per stampa a resina vs filamento. |
| **`category`** | `miniature`, `scenery`, `creature_vehicle`, `bust_statue`, `base_bit`, `functional` | *"pupazzetti"*, *"scenari"*, *"busti"*, *"organizzatori/staffa"*. |
| **`theme`** | `fantasy`, `scifi`, `grimdark`, `historical`, `horror`, `cyberpunk_modern`, `anime_pop`, `everyday_tech` | *"fantasy"*, *"fantascienza"*, *"medievale"*, *"post-apocalittico"*. |
| **`support_state`** | `presupported`, `unsupported`, `mixed`, `slicer_project` | Capire al volo se un pack ha già i supporti pronti o è nudo. |
| **`scale`** | `28mm`, `32mm`, `75mm`, `1_10`, `1_6`, `terrain_28mm` | Identificare la scala di gioco o esposizione. |
| **`creator`** | Slug dedotto (es. `artisan_guild`, `onepagerules`, `loot_studios`...) | Raggruppare automaticamente per autore/Patreon/studio. |

---

## 3. Pipeline di Classificazione Multi-Segnale

Per ogni `model` vengono raccolti e normalizzati i token provenienti da 4 livelli gerarchici, assegnando un peso differente in base alla prossimità e all'affidabilità del segnale:

### 3.1 Segnale 1 — Ereditarietà Gerarchica dal Path
Il motore cammina dalla cartella radice fino al nome del modello/archivio:
- **Cartelle radice / Macro-cartelle (Priorità contestuale forte per `print_tech` e macro-`theme`):**
  - Se un antenato nel path corrisponde a `resina`, `resin`, `sla`, `msla`, `elegoo`, `saturn`, `mars`, `lychee` $\rightarrow$ forte segnale `print_tech: resin` (`+0.90`).
  - Se un antenato corrisponde a `fdm`, `filament`, `filamento`, `bambu`, `pla`, `petg`, `voron`, `prusa`, `a1`, `p1s`, `x1c` $\rightarrow$ forte segnale `print_tech: fdm` (`+0.90`).
- **Cartelle intermedie (Creator / Collezione / Release):**
  - Riconoscimento pattern di data release (`2024-11`, `Nov24`, `Vol.12`) separati dai token semantici.
  - Confronto contro il dizionario dei creator noti (es. `Loot Studios`, `Artisan Guild`, `Archvillain`, `OnePageRules`, `StationForge`, `Printables`, `MakerWorld`, `Gridfinity`), che spesso portano con sé sia `creator` che un prior fortissimo su `theme` e `print_tech`.
- **Override locale lungo il path:** Un segnale più vicino alla foglia (es. sottocartella `FDM_Terrain` dentro una radice generica) vince su eventuali prior più deboli a monte.

### 3.2 Segnale 2 — Euristiche Strutturali sui File Contenuti
Anche se il nome della cartella è povero, la struttura interna dei file rivela moltissimo:
- Presenza di `.lys` / `.lyt`, oppure di file/sottocartelle con token `presupported`, `pre-supported`, `ps_`, `supp`, `hollow`, `chitu` $\rightarrow$ forte segnale **`print_tech: resin`** + **`support_state: presupported`**.
- Presenza di `.3mf` con `Metadata/plate_*.png` o file `.gcode` $\rightarrow$ forte segnale **`print_tech: fdm`**.
- **Distinzione strutturale `miniature` vs `scenery` vs `functional`:**
  - Molti file chiamati `wall`, `floor`, `tile`, `roof`, `pillar`, `ruin`, `door`, `gate`, `bunker`, `corridor`, `openlock` $\rightarrow$ **`category: scenery`**.
  - File chiamati `head`, `torso`, `cape`, `sword`, `shield`, `pose_a`, `base_25mm`, `hero` $\rightarrow$ **`category: miniature`**.
  - File chiamati `bust`, `plinth`, `pedestal` (senza gambe/basette da wargame) $\rightarrow$ **`category: bust_statue`**.
  - File chiamati `bracket`, `holder`, `mount`, `tray`, `insert`, `spool`, `gridfinity`, `skadis`, `case`, `hinge`, `knob`, `organizer` $\rightarrow$ **`category: functional`**.

### 3.3 Segnale 3 — Pulizia Nome e Deduzione del Titolo (`display_title`)
Un modello chiamato `AG_2025_03_Dwarves_King_Thror_32mm_Presupported_v2.zip` viene trasformato automaticamente:
1. **Estrazione segnali:** `creator = Artisan Guild` (`AG`), `theme = fantasy` (`Dwarves`, `King`), `category = miniature`, `scale = 32mm`, `support_state = presupported`.
2. **Rimozione del rumore tecnico:** Vengono rimossi dal titolo mostrato in card prefissi/suffissi come `presupported`, `unsupported`, `v2`, `32mm`, estensioni, underscore e codici studio ripetitivi.
3. **Risultato leggibile in UI:** **`King Thror (Dwarves)`**, mentre tutti i metadati tecnici restano puliti nei badge e nei filtri.

### 3.4 Segnale 4 — Soglia di Confidenza e Fallback
- Ogni coppia `(model, tag)` riceve un punteggio normalizzato $[0.0, 1.0]$ e salva il motivo in `evidence_json` (es. *"ereditato da cartella `/Resina/Fantasy`"*) così l'utente può vedere **perché** il programma ha scelto quel tag.
- Sotto la soglia di confidenza (`< 0.55`), il tag non viene applicato per evitare falsi positivi rumore.

---

## 4. Ricerca Naturale Bilingue (Italiano <-> Inglese)

I modelli su disco hanno quasi sempre nomi inglesi, ma l'utente cerca in italiano naturale. Il parser query espande automaticamente i concetti italiani nei rispettivi filtri a facce + sinonimi inglesi su `models_fts`:

| Espressione utente (IT) | Risoluzione automatica (`facet` + espansione token FTS5) |
| :--- | :--- |
| *"pupazzetti"*, *"omini"*, *"personaggi"*, *"miniature"* | `category:miniature` OR tokens `(miniature, figurine, hero, warrior, character, squad, infantry)` |
| *"scenario"*, *"scenari"*, *"terreni"*, *"edifici"*, *"rovine"* | `category:scenery` OR tokens `(terrain, scenery, ruin, building, wall, dungeon, tabletop, tile)` |
| *"fantascienza"*, *"spaziale"*, *"futuristico"* | `theme:scifi` OR tokens `(scifi, sci-fi, space, starship, mech, cyber, marine, alien, galaxy)` |
| *"fantasy"*, *"medioevo"*, *"mostri"*, *"draghi"* | `theme:fantasy` OR tokens `(fantasy, dragon, elf, dwarf, orc, knight, wizard, goblin, dungeon)` |
| *"resina"* / *"filamento"* | `print_tech:resin` / `print_tech:fdm` |
| *"già supportato"*, *"con supporti"* | `support_state:presupported` |

---

## 5. Correzione Inline (`user_override`) e Evoluzione Futura

- **Correzione in UI:** Nell'Inspector di un modello l'utente può cliccare `×` su un tag sbagliato (imposta `is_excluded = 1` + `origin = 'user_override'`) oppure `+` per correggere scegliendo una delle facce esistenti (`origin = 'user_override'`).
- **Persistenza:** Tutte le correzioni sono replicate in `fingerprint_overrides` indicizzate sull'hash aggregato dei file: anche spostando la cartella o ricostruendo l'indice semantico con una nuova versione del motore, le correzioni dell'utente non vanno mai perse.
- **Hook v2 (AI Vision Locale):** Lo stesso pipeline a punteggio prevede uno slot `Segnale 5 (v2): CLIP/ONNX Vision Classifier` che analizza localmente le miniature `.webp` già generate per classificare pack con nomi completamente opachi (es. `Archivio_01.zip`).
