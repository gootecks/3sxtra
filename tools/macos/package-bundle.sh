#!/usr/bin/env bash
# Builds the combined macOS app "3SXtra.app": the Tauri launcher (universal)
# with a self-contained engine embedded at Contents/Resources/engine/3sx.app.
#
# Usage: tools/macos/package-bundle.sh --engine <path/to/3sx.app> [--out dist/macos]
#
# Produces <out>/3SXtra.app, <out>/3SXtra-<sha>-macos-universal.zip and .dmg.
# Signing is ad-hoc (inner engine first, then the outer app).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE=""
OUT="$ROOT/dist/macos"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --engine) ENGINE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$ENGINE" ]] || { echo "--engine <path/to/3sx.app> is required" >&2; exit 2; }
ENGINE="$(cd "$ENGINE" && pwd)"
[[ -x "$ENGINE/Contents/MacOS/3sx" ]] || { echo "not an engine app: $ENGINE" >&2; exit 1; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

SHA="$(git -C "$ROOT" rev-parse --short HEAD)"
LAUNCHER="$ROOT/tools/launcher"
TARGET=universal-apple-darwin

echo "==> Rust targets"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

echo "==> Frontend"
cd "$LAUNCHER"
if command -v bun >/dev/null 2>&1; then
  # The repo tracks package-lock.json only; don't leave a bun.lock behind.
  had_lock=0; [[ -e bun.lock ]] && had_lock=1
  bun install --no-save
  [[ $had_lock == 1 ]] || rm -f bun.lock
  bun run build
  TAURI=(bunx tauri)
else
  npm ci
  npm run build
  TAURI=(npx tauri)
fi

echo "==> Launcher ($TARGET)"
# Frontend is already built above; skip tauri's beforeBuildCommand.
"${TAURI[@]}" build --target "$TARGET" --bundles app \
  --config '{"build":{"beforeBuildCommand":""}}'

BUILT="$LAUNCHER/src-tauri/target/$TARGET/release/bundle/macos/3SXtra.app"
[[ -d "$BUILT" ]] || { echo "launcher bundle not found: $BUILT" >&2; exit 1; }

APP="$OUT/3SXtra.app"
rm -rf "$APP"
ditto "$BUILT" "$APP"

echo "==> Embedding engine"
mkdir -p "$APP/Contents/Resources/engine"
# rsync rather than ditto so VCS metadata (e.g. a cloned shader repo's .git
# with read-only packs) never ends up in the signed bundle.
rsync -a --exclude '.git' "$ENGINE/" "$APP/Contents/Resources/engine/3sx.app/"
xattr -cr "$APP"

echo "==> Ad-hoc signing"
codesign --force --deep --sign - "$APP/Contents/Resources/engine/3sx.app"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"

EXE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
echo "launcher archs: $(lipo -archs "$APP/Contents/MacOS/$EXE")"
echo "engine archs:   $(lipo -archs "$APP/Contents/Resources/engine/3sx.app/Contents/MacOS/3sx")"

echo "==> Archives"
ZIP="$OUT/3SXtra-$SHA-macos-universal.zip"
DMG="$OUT/3SXtra-$SHA-macos-universal.dmg"
rm -f "$ZIP" "$DMG"
ditto -c -k --keepParent "$APP" "$ZIP"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/3SXtra.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname 3SXtra -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null

echo "Done:"
ls -lh "$ZIP" "$DMG"
echo "$APP"
