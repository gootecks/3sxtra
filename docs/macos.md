# macOS: 3SXtra.app

On macOS the launcher and the engine ship as one app, `3SXtra.app`:

```
3SXtra.app/
  Contents/MacOS/<launcher>              Tauri launcher (universal arm64 + x86_64)
  Contents/Resources/engine/3sx.app      self-contained engine (universal)
  Contents/Resources/engine/3sx.app/Contents/Resources/ENGINE_VERSION
```

`ENGINE_VERSION` line 1 is the UTC ISO build time, line 2 the git short sha.

## Building

Engine first (see [building.md](building.md)), then:

```sh
tools/macos/package-bundle.sh --engine dist/macos/3sx.app [--out dist/macos]
```

This builds the frontend (bun if present, else `npm ci`), runs
`tauri build --target universal-apple-darwin --bundles app`, embeds the
engine, ad-hoc signs (engine first, then the outer app) and writes
`3SXtra-<sha>-macos-universal.zip` and `.dmg`.

CI: `.github/workflows/build_macos_bundle.yml` (`workflow_call` /
`workflow_dispatch`) consumes the `release-assets-macos` artifact from
`build_macos.yml` in the same run and uploads `release-assets-macos-bundle`.

The launcher's release profile keeps host proc-macros unstripped
(`[profile.release.build-override] strip = false`): stripped proc-macro
dylibs fail to `dlopen` on recent macOS ("mis-aligned LINKEDIT string pool").

## Engine discovery

The launcher uses the newest engine among, in order:

1. `<pref>/engine/3sx.app` (installed by the launcher's updater)
2. the embedded `Contents/Resources/engine/3sx.app`
3. `3sx.app` beside `3SXtra.app`
4. `/Applications/3sx.app`
5. `~/Applications/3sx.app`

"Newest" compares `ENGINE_VERSION` line 1; a missing file counts as oldest,
ties prefer the earlier entry. `<pref>` is
`~/Library/Application Support/CrowdedStreet/3SX` (the ROM, config and logs
use the portable `config/` directory instead when one exists, as the engine does).

The updater only downloads `3SX-<sha>-macos-universal.zip` engine assets from
gootecks/3sxtra releases, unpacks them with `ditto` into `<pref>/engine` and
strips quarantine. The launcher itself updates by replacing `3SXtra.app`.

## ROM (SF33RD.AFS)

The ROM panel looks for `SF33RD.AFS` (case-insensitive) in:

1. `<pref>/resources/`
2. each known engine's portable `config/resources/` and `rom/`
3. 3sxw layouts: `resources/` beside a `3SX*.app`, inside it, and `3sxw*` folders
4. beside and inside the launcher app
5. `~/Downloads`, `~/Documents`, `~/Desktop`, `~/Games`, `~/ROMs` (depth ≤ 2,
   also `*.iso`)
6. mounted discs: `/Volumes/*/THIRD/SF33RD.AFS`, `/Volumes/*/SF33RD.AFS`

Import copies the file (or extracts `THIRD/SF33RD.AFS` from an ISO via
`hdiutil`) into `<pref>/resources/` and reports whether its SHA-256 matches the
known dump. The engine's own folder picker accepts a folder containing
`THIRD/SF33RD.AFS` or `SF33RD.AFS`.

## Support commands

```sh
3SXtra.app/Contents/MacOS/<launcher> --diagnose   # JSON: engine candidates, chosen engine, ROM status
3SXtra.app/Contents/MacOS/<launcher> --launch     # start the engine without the UI
```

The engine's stdout/stderr go to `<pref>/logs/engine-stdout.log` and
`engine-stderr.log`.
