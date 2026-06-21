# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Project

Lore Desktop — a multiplatform **Tauri 2** (Rust) + **React 18 / TypeScript / Tailwind v4**
desktop client for [Lore](https://github.com/EpicGames/lore), Epic Games' open-source,
binary-first version control system. Targets macOS, Windows, and Linux.

- Frontend: `src/` (React + Zustand, talks to the backend only through `src/api/lore.ts`).
- Backend: `src-tauri/src/` (Tauri commands, the `lore/` integration layer, daemon, lock manager).
- Data contracts are mirrored 1:1 across `src/types/lore.ts` and `src-tauri/src/models.rs`.
- Backend selection: real `lore` CLI when `LORE_BIN` + `LORE_REPOSITORY` are set, else a
  stateful mock. The `liblore` FFI path is reserved behind the `liblore` cargo feature.

## Commands

```bash
npm install                       # install frontend deps
npm run tauri dev                 # run the app (Vite + Tauri)
npm run typecheck                 # tsc --noEmit
npm run build                     # tsc --noEmit && vite build
(cd src-tauri && cargo test --lib)   # compile backend + run unit tests (parsers)
(cd src-tauri && cargo check)        # fast backend type-check
```

## Required workflow: test → commit → push

Always run local testing **before** committing. Only commit if everything passes, and only
push **after** a successful commit. Do not skip the tests, and do not push a commit whose
pre-commit checks did not pass.

**1. Local testing (must pass before any commit):**

```bash
npm run typecheck                       # frontend type-check
(cd src-tauri && cargo test --lib)      # backend compile + unit tests
```

Run `npm run build` as well when the change touches the frontend build (bundling/aliases),
and exercise the app with `npm run tauri dev` when the change affects runtime behavior.

- If any check fails, **do not commit** — fix the failure first, then re-run the checks.
- Report the actual results honestly (e.g. "6 tests passed", or the failing output).

**2. Commit** — only once step 1 passes clean. End commit messages with:

```
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

**3. Push** — only after the commit succeeds. Confirm a remote/branch exists first; if the
branch is the default branch, branch before pushing unless this is an initial commit.

### Enforcement: pre-commit hook

This workflow is enforced by a version-controlled git hook at `.githooks/pre-commit`. It is
change-aware: it runs the frontend typecheck only when frontend files are staged, and
`cargo test --lib` only when Rust files are staged, then aborts the commit if anything fails.

One-time setup (already configured in this repo; required after a fresh clone):

```bash
git config core.hooksPath .githooks
```

- The hook is the safety net; still run the step-1 checks yourself — don't rely on it to
  catch problems late.
- Bypass only in a genuine emergency with `git commit --no-verify`, and say so explicitly.

## Conventions

- The UI must never bypass `src/api/lore.ts`; the backend must never bypass the `LoreClient`
  trait — this seam is what lets the CLI backend be swapped for `liblore` later.
- CLI-output parsers in `src-tauri/src/lore/parse.rs` are unit-tested against real `lore`
  output; update the tests with captured output, never guessed formats.
- Keep `src/types/lore.ts` and `src-tauri/src/models.rs` in sync when changing contracts.

## Known advisories (accepted, upstream-blocked)

- **`glib` < 0.20 — RUSTSEC / GHSA-wrw7-89jp-8q8g** (medium, Dependabot alert #5):
  unsoundness in `glib::VariantStrIter` iterator impls. **No fix available to us yet.**
  `glib 0.18` is pinned transitively by `gtk 0.18` ← `webkit2gtk`/`wry` ← `tauri 2.11.3`
  (the latest release); `cargo update --precise 0.20.0` fails the `gtk = "^0.18"` constraint,
  and a `[patch]` to 0.20 breaks compilation (0.18→0.20 is a breaking API change).
  Linux-only (macOS/Windows don't compile `glib`); our code never calls `glib` directly.
  **Resolution path:** will clear automatically once Tauri/wry move their GTK backend to
  gtk-rs 0.20+ — re-check after any Tauri bump with
  `cargo update -p glib --precise 0.20.0` (success = fix is available; then bump for real).
