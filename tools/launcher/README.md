# 3SX Launcher

Tauri 2 + React launcher for 3SX: installs/updates the engine from
gootecks/3sxtra releases, edits the flat `config` (renderer, GPU driver, …)
and key mappings, finds and imports the `SF33RD.AFS` ROM, and launches the game.

## Development

```sh
npm ci            # or: bun install --no-save (don't commit bun.lock)
npm run tauri dev
cd src-tauri && cargo test
```

Backend modules (`src-tauri/src/`): `lib.rs` (Tauri commands, `--diagnose`,
`--launch`), `paths.rs` (pref/portable dirs), `engine.rs` (macOS engine
discovery and launch), `rom.rs` (ROM probe/import), `updater.rs` (release
download and install).

## macOS

On macOS the launcher ships as `3SXtra.app` with the engine embedded; see
[docs/macos.md](../../docs/macos.md) and `tools/macos/package-bundle.sh`.
