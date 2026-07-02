# Lore Desktop

A high-performance, multiplatform desktop client for **[Lore](https://github.com/EpicGames/lore)** — Epic Games' open-source, content-addressed, binary-first version control system (open-sourced June 17, 2026; formerly Unreal Revision Control).

Built with **Tauri 2** (Rust backend) + **React 18 / TypeScript / Tailwind v4**. Targets macOS, Windows, and Linux from one codebase.

## Why this client

Lore is binary-first: massive `.uasset` / `.umap` files, content-defined chunking into **fragments**, sparse hydration, and **locks for unmergeable content**. This client emphasizes the binary workflow — *is this asset locked, by whom, checked out, or modified* — over text diffs.

## Status — Phase 1 (scaffolding & data contracts)

| Layer | What's here |
|---|---|
| Data contracts | [`src/types/lore.ts`](src/types/lore.ts) ↔ [`src-tauri/src/models.rs`](src-tauri/src/models.rs) — Revision, Fragment, Branch, Workspace (Lore *Instance*), Lock, FileEntry, events. Grounded in Lore's real system design. |
| Mock backend | [`src-tauri/src/commands.rs`](src-tauri/src/commands.rs) + [`mock.rs`](src-tauri/src/mock.rs) return static JSON so the UI develops in isolation from `liblore`. Lock mutations emit a `lore://event`. |
| IPC seam | [`src/api/lore.ts`](src/api/lore.ts) — the only place the UI touches `invoke`/`listen`. Phase 2 swaps command bodies for liblore without touching the UI. |
| Demo UI | [`src/App.tsx`](src/App.tsx) — binary-first file list with live lock acquire/release round-tripping through Rust. |

### Data model mapping (brief term → Lore's actual term)

- Revisions → **Revisions** (hash-identified Merkle snapshots, immutable DAG)
- Fragments → **Fragments** (48-byte address: 32-byte BLAKE3 + 16-byte context tag)
- Workspaces → **Instances** (UUIDv7 working dirs; shown as "Workspace" in UI)
- File Locks → **Locks** (`lore lock acquire | status | release`)

## Develop

```bash
npm install
npm run tauri dev      # runs Vite + Tauri (mock backend)
npm run typecheck      # tsc --noEmit
(cd src-tauri && cargo check)
```

The backend defaults to **mock data**. A `liblore` cargo feature is reserved for the Phase 2 real integration.

### liblore FFI backend (optional)

```bash
scripts/fetch-liblore.sh                                # shared library for this machine
(cd src-tauri && cargo test --lib --features liblore)   # build + test against it
```

Supported targets — every platform upstream publishes a liblore build for, verified by
CI on each PR: **macOS arm64**, **Linux x86_64 / aarch64**, **Windows x86_64**.
Intel macOS has no upstream liblore build; the default CLI backend covers it there.
The target → archive/library mapping lives in
[`src-tauri/liblore_layout.rs`](src-tauri/liblore_layout.rs) (shared by `build.rs` and
unit tests) and [`scripts/fetch-liblore.sh`](scripts/fetch-liblore.sh).

## Roadmap

- **Phase 2** — Rust ↔ Lore integration (liblore FFI vs. CLI), daemon lifecycle controller (`lore service`), binary lock manager.
- **Phase 3** — Full binary-first React UI, event-driven live state.
- **Phase 4** — Native binary diff tool hooks, memory-efficient multi-GB streaming.
- **Phase 5** — ✅ Security hardening (defense-in-depth; no known exploitable issue before it, single-user/local-trust model):
  - **Content-Security-Policy enabled.** `app.security.csp` in [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) is a strict bundled-assets-only policy (plus a `devCsp` that additionally allows the Vite dev server's HMR websocket), so any future injected content can't reach the IPC bridge.
  - **Path & URL inputs validated at the backend boundary.** Caller-supplied repository paths go through [`src-tauri/src/paths.rs`](src-tauri/src/paths.rs) (`resolve_repo_path`): relative-only, no `..`, and a canonicalized containment check that also catches symlinks pointing outside the repository — used by `launch_asset_diff` and `stream_ingest_file`. `lore clone` option parsing is terminated with `--` so a leading-`-` URL/path can't be read as a flag.
  - **Asset-diff temp file hardened.** The base snapshot in `launch_asset_diff` is created by the `tempfile` crate (exclusive create, unpredictable name) instead of a predictable `temp_dir().join(...)` path.
