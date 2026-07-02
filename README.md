# Lore Desktop

A high-performance, multiplatform desktop client for **[Lore](https://github.com/EpicGames/lore)** — Epic Games' open-source, content-addressed, binary-first version control system (open-sourced June 17, 2026; formerly Unreal Revision Control).

Built with **Tauri 2** (Rust backend) + **React 18 / TypeScript / Tailwind v4**. Targets macOS, Windows, and Linux from one codebase.

## Why this client

Lore is binary-first: massive `.uasset` / `.umap` files, content-defined chunking into **fragments**, sparse hydration, and **locks for unmergeable content**. This client emphasizes the binary workflow — *is this asset locked, by whom, checked out, or modified* — over text diffs.

## Status

All planned phases (1–5) are complete and merged, with CI green on every supported target. The app is a working GitHub Desktop-style client that runs against a real Lore repository through either the `lore` CLI or an in-process `liblore` FFI binding, and falls back to a stateful mock when no repository is configured.

### What it does today

- **Repository workflow** — open or clone a repository at runtime (native folder picker), switch/create branches, sync to a revision, push, stage/unstage, and commit.
- **Binary-first changes view** — file list keyed on asset kind and lock state rather than text diffs; the detail pane shows asset metadata + lock disposition and launches a native visual diff tool (FileMerge, P4Merge, Beyond Compare, VS Code — auto-detected per platform).
- **Lock management** — acquire/release exclusive locks on unmergeable assets through a single [`LockManager`](src-tauri/src/lock_manager.rs) choke point; locks are attributed *you vs. others* via `lore auth info`, and every mutation emits an event so all views refresh instantly.
- **Live updates** — a cross-platform, debounced filesystem watcher ([`daemon.rs`](src-tauri/src/daemon.rs)) auto-refreshes status on disk changes and periodically re-queries server-side locks (`lore service run` is not usable on macOS in 0.8.3, so the client ships its own watcher).
- **History** — revision history with real commit messages, hashes, and timestamps.
- **Streaming ingest** — hashes multi-gigabyte assets into BLAKE3 content-addressed fragments with a single fixed-size buffer resident ([`streaming.rs`](src-tauri/src/streaming.rs)), pushing progress events to the UI (demoed: 1 GiB file → 256 fragments at ~4 MiB peak RAM).
- **Security hardening (Phase 5)** — strict bundled-assets-only CSP, backend-boundary path validation ([`paths.rs`](src-tauri/src/paths.rs): relative-only, no `..`, canonicalized containment that catches symlink escapes), `--`-terminated `lore clone` argument parsing, and `tempfile`-based asset-diff snapshots.

## Architecture

```
React UI (src/)  ──  src/api/lore.ts  ──  Tauri IPC  ──  commands.rs  ──  LoreClient trait
                     (the only IPC seam)                                   ├─ CliLoreClient   (wraps the real `lore` CLI)
                                                                           ├─ FfiLoreClient   (in-process liblore, feature-gated)
                                                                           └─ MockLoreClient  (stateful; default when unconfigured)
```

- The UI never bypasses [`src/api/lore.ts`](src/api/lore.ts); the backend never bypasses the [`LoreClient`](src-tauri/src/lore/mod.rs) trait. This seam is what made the CLI → FFI swap a one-line change in `build_client`.
- Data contracts are mirrored 1:1 across [`src/types/lore.ts`](src/types/lore.ts) and [`src-tauri/src/models.rs`](src-tauri/src/models.rs).
- CLI-output parsers ([`parse.rs`](src-tauri/src/lore/parse.rs)) are unit-tested against captured output from a real `lore 0.8.3` binary, never guessed formats.
- Backend selection at startup: `liblore` FFI when built with the feature and a repository is configured; otherwise the real CLI when `LORE_BIN`/PATH + `LORE_REPOSITORY` resolve; otherwise the mock. The active mode is surfaced in the toolbar as a **live · liblore** / **live · lore CLI** / **mock data** badge.

### Data model mapping (UI term → Lore's actual term)

- Revisions → **Revisions** (hash-identified Merkle snapshots, immutable DAG)
- Fragments → **Fragments** (48-byte address: 32-byte BLAKE3 + 16-byte context tag)
- Workspaces → **Instances** (UUIDv7 working dirs; shown as "Workspace" in UI)
- File Locks → **Locks** (`lore lock acquire | status | release`)

## Develop

```bash
npm install
npm run tauri dev      # runs Vite + Tauri (mock backend unless configured)
npm run typecheck      # tsc --noEmit
npm run build          # tsc --noEmit && vite build
(cd src-tauri && cargo test --lib)   # backend compile + unit tests
```

To run against a real repository, set `LORE_REPOSITORY` (and `LORE_BIN` if `lore` isn't on PATH), or just launch with the mock and use **Open repository…** / **Clone…** from the toolbar at runtime.

### liblore FFI backend (optional)

```bash
scripts/fetch-liblore.sh                                # shared library for this machine
(cd src-tauri && cargo test --lib --features liblore)   # build + test against it
```

The FFI backend binds liblore in-process — no text parsing, no subprocess hangs, structured event-driven results — and implements the full `LoreClient` surface ([`ffi.rs`](src-tauri/src/lore/ffi.rs)).

Supported targets — every platform upstream publishes a liblore build for, verified by CI on each PR: **macOS arm64**, **Linux x86_64 / aarch64**, **Windows x86_64**. Intel macOS has no upstream liblore build; the default CLI backend covers it there. The target → archive/library mapping lives in [`src-tauri/liblore_layout.rs`](src-tauri/liblore_layout.rs) (shared by `build.rs` and unit tests) and [`scripts/fetch-liblore.sh`](scripts/fetch-liblore.sh).

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs on every PR across a 4-target matrix (macOS arm64, Linux x86_64, Linux aarch64, Windows x86_64): frontend typecheck + build, `cargo test --lib`, then `scripts/fetch-liblore.sh` + `cargo test --lib --features liblore` — including a live `lore_version()` FFI call on every target.

## Known advisories

- **`glib` < 0.20** (RUSTSEC, medium; Linux-only, transitive via `gtk 0.18` ← `wry` ← Tauri): no fix available until Tauri/wry move to gtk-rs 0.20+. Accepted and monitored — details in [CLAUDE.md](CLAUDE.md).
