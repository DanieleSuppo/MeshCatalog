<!--
Sync Impact Report:
- Version change: template -> 1.0.0
- Modified principles: initial ratification of all 5 Core Principles (I–V)
- Added sections: Technical Stack & Cross-OS Constraints, Master Architecture Reference, Governance
- Removed sections: None
- Follow-up TODOs: None
-->

# MeshCatalog Constitution

## Core Principles

### I. Read-Only User Filesystem (Zero Sidecars)
In v1, the indexer, background workers, and UI **MUST NEVER** modify, move, rename, delete, or write hidden/sidecar files inside the user's 3D model directories or Dropbox folders. All application state—including the SQLite database (`catalog.db`), `.webp` thumbnail caches, logs, configuration, and local OAuth tokens—**MUST** reside exclusively within the application data directory (`%LOCALAPPDATA%\MeshCatalog`).
*Rationale:* Protecting ~700 GB of user collections from accidental mutation or cloud sync churn is paramount for user trust and filesystem integrity.

### II. Dropbox Zero-Explosion (Win32 Anti-Hydration Guard)
On Windows, before opening *any* file handle for hashing, parsing, or archive inspection, the scanner **MUST** inspect Win32 file attributes via `std::fs::metadata` / `GetFileAttributesW` without opening a read stream. If either `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (`0x00400000`) or `FILE_ATTRIBUTE_OFFLINE` (`0x00001000`) is set, the file is an *Online-Only* placeholder: the application **MUST NOT** call `File::open` or invoke Windows Shell thumbnail/property APIs that force hydration. Remote Dropbox trees and remote `.zip`/`.3mf` archives **MUST** be inspected exclusively via Dropbox API v2 (`list_folder`, `get_thumbnail_batch`) and `HTTP Range` requests (`206 Partial Content`), with volatile RAM rendering of cloud-only `.stl` files strictly gated behind the explicit user toggle (`render_cloud_stl_in_ram`).
*Rationale:* Triggering implicit NTFS hydration on ~660 GB of Smart Sync files would exhaust local SSD space and bandwidth.

### III. Zero-Latency 2D Browsing vs On-Demand 3D Viewport
Grid scrolling, folder browsing, search, and facet filtering **MUST** read exclusively from SQLite (`WAL` mode + `FTS5`) and the local `.webp` thumbnail cache, targeting a `< 16 ms` frame budget via virtualized rendering (`@tanstack/react-virtual`). Card components in list/grid views **MUST NOT** mount WebGL/Three.js contexts or parse 3D geometry. Interactive 3D rendering (`Three.js` / `@react-three/fiber`) **MUST** be mounted strictly on-demand inside the side Inspector or QuickLook (`Space`).
*Rationale:* Decoupling 2D cached browsing from 3D geometry parsing guarantees instant responsiveness across tens of thousands of models.

### IV. Code-Driven Automatic Bilingual Taxonomy (No Rule-Builder UI)
Catalog classification **MUST** operate 100% automatically via versioned dictionaries, path-inheritance priors, structural file heuristics, and bilingual IT/EN query expansion maintained in repository code/config. The application **MUST NOT** introduce a manual-first tagging workflow or a UI rule/filter configurator. The UI **MUST** expose only lightweight inline tag corrections on a single model (`origin = 'user_override'`, including suppression via `is_excluded = 1`), and all overrides **MUST** survive re-indexing and folder/archive moves via content hashes (`BLAKE3` locally, `content_hash` on Dropbox) and `fingerprint_overrides`.
*Rationale:* Users will not manually tag ~700 GB of assets; semantic accuracy improves cleanly through versioned rules without UI bloat.

### V. Zero Proprietary Licenses & Cross-OS WSL2 <-> Windows Verification
All format parsers, archive readers, renderers, and database engines **MUST** rely exclusively on open-source, royalty-free libraries and open specifications (`.stl`, `.3mf`, `.obj`, `.ply`, `.glb`, `.zip`, plus Tier-1 metadata/thumbnail extraction for `.lys`/`.lyt`). Development and git authoring occur in **WSL2 (Linux)** bridged via GitHub (`DanieleSuppo/MeshCatalog`) to **Windows 10/11** (`x86_64-pc-windows-msvc`). Every milestone **MUST** keep platform-independent Rust core logic unit-tested in WSL2 (`cargo check` / `cargo test`) while gating Win32 attribute and GPU rendering verification on native Windows builds (`cargo tauri dev` / build).
*Rationale:* Ensuring zero licensing fees while keeping rapid cross-OS iteration reliable.

## Technical Stack & Master Architecture Reference

- **Desktop Stack:** **Tauri v2** (Rust backend + `rusqlite` bundled with `FTS5` + `blake3` + `zip` + `reqwest` + headless `wgpu` thumbnailer + `windows-sys`) and **React 19 + TypeScript + Vite + Three.js** (`@react-three/fiber` + `@react-three/drei`).
- **Master Reference Documents:** Every `/speckit-specify`, `/speckit-plan`, `/speckit-tasks`, and `/speckit-implement` cycle **MUST** align with the authoritative global specifications already defined in the repository:
  - [AGENTS.md](../../AGENTS.md) — Architectural invariants & session context
  - [docs/01-PRODUCT-SPEC.md](../../docs/01-PRODUCT-SPEC.md) — Product scope, format matrix, v1 vs v2+ boundaries
  - [docs/02-ARCHITECTURE.md](../../docs/02-ARCHITECTURE.md) — Zero-Explosion Dropbox design, 3-pass pipeline, SQLite DDL
  - [docs/03-TAXONOMY-ENGINE.md](../../docs/03-TAXONOMY-ENGINE.md) — Multi-signal classifier, IT/EN expansion, overrides
  - [docs/04-UI-UX-AND-IPC.md](../../docs/04-UI-UX-AND-IPC.md) — Smart Folder bento previews, 3D Inspector, Tauri IPC contracts
  - [docs/05-ROADMAP.md](../../docs/05-ROADMAP.md) — Milestone breakdown (`M0`–`M5`)

## Governance

- This Constitution supersedes all ad-hoc implementation choices, shortcuts, or conflicting patterns.
- Amendments require an explicit update to `.specify/memory/constitution.md`, a semantic version bump (`MAJOR` for principle removal/redefinition, `MINOR` for new principles or material expansion, `PATCH` for clarifications), and synchronization with `AGENTS.md` if architectural invariants change.
- Every `/speckit-plan` and `/speckit-analyze` gate **MUST** explicitly verify compliance with Principles I–V before implementation begins.

**Version**: 1.0.0 | **Ratified**: 2026-11-26 | **Last Amended**: 2026-11-26
