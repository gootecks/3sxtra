# macOS universal release: plan (2026-10-06)

Epic: gootecks/3sxtra#1. Branch: `feat/1-macos-universal-release`.

| # | Slice | Output |
| --- | --- | --- |
| 2 | Engine macOS fixes | GL 4.1 core, `glTexStorage` fallback, Retina viewport, bundle asset paths |
| 3 | Universal self-contained build + CI | `tools/macos/package-engine.sh`; `3SX-<sha>-macos-universal.{zip,dmg}` |
| 4 | Render driver selection + fallback | `gpu-driver` key / `--gpu-driver`; docs/render-drivers.md |
| 5 | Launcher macOS support | engine discovery, launch, updater via `ditto` |
| 6 | Single combined app | `3SXtra.app` (engine at `Contents/Resources/engine/3sx.app`); `3SXtra-<sha>-macos-universal.{zip,dmg}` |
| 7 | SF33RD.AFS discovery | launcher probe + import; engine folder picker accepts a plain `SF33RD.AFS` |
| 8 | 3sxw comparison | docs/3sxw-comparison.md |

FreeFighter consumes the `3SXtra-*-macos-universal.zip` asset from the
`rolling-pre-release` tag (or the newest `v*` tag).
