# macOS: 3sx.app (engine)

The engine ships as a self-contained `3sx.app`. The launcher lives in the
separate [gootecks/3sx-launcher](https://github.com/gootecks/3sx-launcher)
repository and finds `3sx.app` on its own; FreeFighter downloads the two
independently.

`Contents/Resources/ENGINE_VERSION` line 1 is the UTC ISO build time, line 2
the git short sha; the launcher's updater and discovery compare these.

## Building

```sh
tools/macos/package-engine.sh --arch universal [--build-dir DIR] [--out dist/macos]
```

Produces `3sx.app` (all non-system dylibs in `Contents/Frameworks`,
assets/shaders/lua in `Contents/Resources`), ad-hoc signed, plus
`3SX-<sha>-macos-universal.zip` / `.dmg`. `--arch arm64` / `x86_64` builds a
single-arch variant; universal is the release default.

CI: `.github/workflows/build_macos.yml` (`workflow_call` /
`workflow_dispatch`).

## Releases

macOS ships on its own, independent of the other platforms
(`.github/workflows/release_macos.yml`; `release.yml` no longer builds macOS):

- Every push to `main` (or a manual dispatch) refreshes the `macos-rolling`
  pre-release with `3SX-<sha>-macos-universal.{dmg,zip}` and
  `SHA256SUMS.txt`. New assets are uploaded before old ones are pruned.
- Pushing a `macos-v*` tag also publishes a release for that tag (marked
  pre-release when the tag has a `-` suffix, e.g. `macos-v0.1.0-rc.1`).
- FreeFighter installs `3SX-*-macos-universal.zip` from `macos-rolling`.

## ROM (SF33RD.AFS)

The engine looks for the ROM (case-insensitive) in its portable `config/`
directory, then beside the executable. The launcher's ROM panel (separate
repo) locates it from discs, ISOs and common folders and copies it into
`<pref>/resources/`.

## Support

The engine's stdout/stderr can be tee'd to a log by the launcher; `--help`
lists CLI options.
