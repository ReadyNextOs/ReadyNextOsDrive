# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

ReadyNextOs Drive — desktop file synchronization client. Tauri v2 (Rust backend) + React 18 + TypeScript frontend. Uses rclone bisync sidecar for bidirectional WebDAV sync with a ReadyNextOs server.

## Commands

```bash
npm install                # install frontend dependencies
npm run tauri dev          # full dev environment (Vite on :1420 + Tauri debug window)
npm run tauri build        # production build (bundles frontend + compiles Rust)
npm run build              # frontend only: tsc && vite build
cargo check --manifest-path src-tauri/Cargo.toml   # type-check Rust without full build
cargo clippy --manifest-path src-tauri/Cargo.toml   # Rust linter
```

No test suite exists — no `cargo test` or JS test runner configured.

## Architecture

### IPC bridge

Frontend calls Rust via Tauri commands. The bridge layer is `src/lib/tauri.ts` — typed wrappers around `invoke()`. Corresponding `#[tauri::command]` handlers live in `src-tauri/src/main.rs`. When adding a new command, update both sides and register it in the `.invoke_handler(tauri::generate_handler![...])` call in `main()`.

### Rust modules (`src-tauri/src/`)

| Module | Purpose |
|--------|---------|
| `main.rs` | App setup, all Tauri command handlers, tray menu, event listeners |
| `auth.rs` | Login HTTP call, OS keychain token storage (`keyring` crate) |
| `config.rs` | `AppConfig`, `SyncStatus`, `ActivityEntry` types; persistent config via Tauri store |
| `sync.rs` | `SyncEngine` — orchestrates rclone bisync for personal + shared folders |
| `watcher.rs` | `FileWatcher` — `notify` crate filesystem polling, triggers sync on changes |

### Frontend pages (`src/pages/`)

`App.tsx` renders either `LoginPage` or a tab bar (Status | Activity | Settings). Components call `@/lib/tauri.ts` directly — no global state management or prop drilling.

### Shared state

`AppState` in `main.rs` holds `Mutex<AppConfig>`, `Arc<SyncEngine>`, `Mutex<FileWatcher>`. All command handlers receive it via `State<'_, AppState>`.

### Sync flow

1. Auth token retrieved from OS keychain, obscured via `rclone obscure`
2. rclone bisync runs as sidecar subprocess (credentials passed via env vars, not CLI args)
3. Separate sync for personal (`/remote.php/dav/files/{email}/`) and shared (`/remote.php/dav/groupfolders/`) WebDAV endpoints
4. First-run uses `.readynextos-sync-init` marker file to distinguish `--resync` from incremental
5. `FileWatcher` with 2-second poll can trigger immediate sync on local changes

### Path alias

TypeScript uses `@/` → `src/` (configured in both `tsconfig.json` and `vite.config.ts`).

## Conventions

- UI text is **Polish** — maintain `pl-PL` for all user-facing strings
- Rust errors are `Result<T, String>` — convert errors to descriptive strings at the command boundary
- Credentials never appear in logs or process args — use env vars for rclone auth
- Default sync paths: `~/ReadyNextOs/Moje pliki` (personal), `~/ReadyNextOs/Udostępnione` (shared)
- Version is tracked in three places: `package.json`, `Cargo.toml`, `tauri.conf.json`

## CI/CD

GitHub Actions (`.github/workflows/build.yml`) builds on push to `main` and manual dispatch. Produces `.deb`, `.rpm`, `.AppImage` (Linux), `.msi` (Windows), `.dmg` (macOS ARM + Intel). The workflow downloads rclone v1.68.2 and generates placeholder icons before building.

## Prerequisites (Linux dev)

```bash
sudo pacman -S webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf  # Arch/Manjaro
```
