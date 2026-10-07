#!/usr/bin/env bash
# Build and package a self-contained, ad-hoc signed 3sx.app for macOS.
#
# Usage: tools/macos/package-engine.sh [--arch universal|arm64|x86_64] [--build-dir DIR]
#                                      [--out DIR] [--build-type TYPE] [--vulkan-sdk DIR]
#
# Produces in --out (default dist/macos):
#   3sx.app                              all non-system dylibs in Contents/Frameworks,
#                                        assets/shaders/lua copied into Contents/Resources
#   3SX-<sha>-macos-<arch>.zip / .dmg    release archives (dmg has an /Applications link)
#
# Extra CMake arguments can be passed via PACKAGE_CMAKE_ARGS (e.g. compiler launchers).
# The bundle is ad-hoc signed only; Developer ID signing + notarisation are not done here.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
ARCH="universal"
BUILD_DIR=""
OUT_DIR="$ROOT_DIR/dist/macos"
BUILD_TYPE="Release"
VULKAN_SDK_DIR=""

usage() { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }
die() { echo "package-engine: ERROR: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --arch) ARCH="${2:?}"; shift 2 ;;
        --build-dir) BUILD_DIR="${2:?}"; shift 2 ;;
        --out) OUT_DIR="${2:?}"; shift 2 ;;
        --build-type) BUILD_TYPE="${2:?}"; shift 2 ;;
        --vulkan-sdk) VULKAN_SDK_DIR="${2:?}"; shift 2 ;;
        -h|--help) usage 0 ;;
        *) echo "Unknown argument: $1" >&2; usage 1 ;;
    esac
done

[ "$(uname -s)" = "Darwin" ] || die "macOS only"
case "$ARCH" in
    universal) WANT_ARCHS="arm64 x86_64"; CMAKE_ARCHS="arm64;x86_64" ;;
    arm64|x86_64) WANT_ARCHS="$ARCH"; CMAKE_ARCHS="$ARCH" ;;
    *) die "--arch must be universal, arm64 or x86_64 (got '$ARCH')" ;;
esac
BUILD_DIR="${BUILD_DIR:-$ROOT_DIR/build-macos-$ARCH}"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"
case "$BUILD_DIR" in /*) ;; *) BUILD_DIR="$PWD/$BUILD_DIR" ;; esac

SHA="$(git -C "$ROOT_DIR" rev-parse --short=7 HEAD)"
BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
# CFBundleShortVersionString: release tag (vX.Y.Z -> X.Y.Z) when HEAD is tagged, else 0.0.0.
TAG="$(git -C "$ROOT_DIR" describe --tags --exact-match --match 'v[0-9]*' 2>/dev/null || true)"
BUNDLE_VERSION="0.0.0"
if [[ "$TAG" =~ ^v([0-9]+(\.[0-9]+){0,2}) ]]; then BUNDLE_VERSION="${BASH_REMATCH[1]}"; fi

export TARGET_ARCH="$ARCH" MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
GENERATOR=()
if [ ! -f "$BUILD_DIR/CMakeCache.txt" ] && command -v ninja >/dev/null; then GENERATOR=(-G Ninja); fi

echo "==> Configuring ($ARCH, $BUILD_TYPE) in $BUILD_DIR"
# shellcheck disable=SC2086
cmake -S "$ROOT_DIR" -B "$BUILD_DIR" ${GENERATOR[@]+"${GENERATOR[@]}"} \
    -DCMAKE_BUILD_TYPE="$BUILD_TYPE" \
    -DCMAKE_OSX_ARCHITECTURES="$CMAKE_ARCHS" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET" \
    -DMACOS_BUNDLE_VERSION="$BUNDLE_VERSION" \
    -DENABLE_TESTS=OFF \
    -DINSTALL_ASSETS=OFF \
    ${PACKAGE_CMAKE_ARGS:-}

echo "==> Building"
cmake --build "$BUILD_DIR" --parallel

SRC_APP="$BUILD_DIR/3sx.app"
APP="$OUT_DIR/3sx.app"
FW="$APP/Contents/Frameworks"
MACOS="$APP/Contents/MacOS"
RES="$APP/Contents/Resources"
EXE="$MACOS/3sx"
[ -x "$SRC_APP/Contents/MacOS/3sx" ] || die "build did not produce $SRC_APP"

echo "==> Assembling $APP"
rm -rf "$APP"
# --copy-links turns the build-tree symlinks (assets -> source tree, libretro -> third_party)
# into real files; VCS metadata is not shipped.
rsync -a --copy-links --exclude '.git' --exclude '.DS_Store' "$SRC_APP/" "$APP/"
mkdir -p "$FW"
printf '%s\n%s\n' "$BUILD_TIME" "$SHA" > "$RES/ENGINE_VERSION"

# ---------------------------------------------------------------------------
# Dylib bundling
# ---------------------------------------------------------------------------

is_system() { case "$1" in /usr/lib/*|/System/*) return 0 ;; *) return 1 ;; esac; }
# Fat binaries list load commands once per slice: skip "(architecture …):" headers, de-duplicate.
deps_of() { otool -L "$1" | awk '!/:$/ && !seen[$1]++ {print $1}'; }
rpaths_of() { otool -l "$1" | awk '$1=="cmd" && $2=="LC_RPATH" {getline; getline; if (!seen[$2]++) print $2}'; }
install_id() { otool -D "$1" | awk 'NR==2 {print $1}'; }

# resolve_dep <binary> <original-binary-dir> <dep>: absolute path of a non-system dependency.
resolve_dep() {
    local bin="$1" origin="$2" dep="$3" name rp cand
    case "$dep" in
        @rpath/*)
            name="${dep#@rpath/}"
            [ -f "$FW/$name" ] && { echo "$FW/$name"; return; }
            # A library's own rpaths first, then the build-tree executable's (third-party dylibs
            # are often built without LC_RPATH and rely on the loading executable's).
            for rp in $(rpaths_of "$bin") $(rpaths_of "$SRC_APP/Contents/MacOS/3sx"); do
                rp="${rp/#@loader_path/$origin}"
                rp="${rp/#@executable_path/$(dirname "$SRC_APP/Contents/MacOS/3sx")}"
                cand="$rp/$name"
                [ -f "$cand" ] && { echo "$cand"; return; }
            done ;;
        @loader_path/*) cand="$origin/${dep#@loader_path/}"; [ -f "$cand" ] && { echo "$cand"; return; } ;;
        @executable_path/*) cand="$SRC_APP/Contents/MacOS/${dep#@executable_path/}"; [ -f "$cand" ] && { echo "$cand"; return; } ;;
        /*) [ -f "$dep" ] && { echo "$dep"; return; } ;;
    esac
    return 1
}

# bundle_deps <bundled-binary> <origin-dir>: copy its non-system deps into Frameworks
# (recursively) and point every reference at @rpath/<name>.
bundle_deps() {
    local bin="$1" origin="$2" dep src name self
    self="$(install_id "$bin" || true)"
    for dep in $(deps_of "$bin"); do
        is_system "$dep" && continue
        [ "$dep" = "$self" ] && continue
        name="$(basename "$dep")"
        if [ ! -f "$FW/$name" ]; then
            src="$(resolve_dep "$bin" "$origin" "$dep")" || die "cannot resolve $dep (needed by $bin)"
            echo "    + $name  <- $src"
            cp -L "$src" "$FW/$name"
            chmod u+w "$FW/$name"
            install_name_tool -id "@rpath/$name" "$FW/$name" 2>/dev/null
            bundle_deps "$FW/$name" "$(dirname "$src")"
            fix_rpaths "$FW/$name" "@loader_path"
        fi
        [ "$dep" = "@rpath/$name" ] || install_name_tool -change "$dep" "@rpath/$name" "$bin" 2>/dev/null
    done
}

# fix_rpaths <binary> <rpath>: drop every LC_RPATH and leave exactly <rpath>.
fix_rpaths() {
    local bin="$1" want="$2" rp
    for rp in $(rpaths_of "$bin"); do install_name_tool -delete_rpath "$rp" "$bin" 2>/dev/null; done
    install_name_tool -add_rpath "$want" "$bin" 2>/dev/null
}

# Optional Vulkan loader + MoltenVK ICD (another component chooses the driver at runtime).
bundle_vulkan() {
    local sdk="$1" dir lib found manifest api="1.2.0"
    for lib in libvulkan.1.dylib libMoltenVK.dylib; do
        found=""
        for dir in "$sdk/lib" "$sdk/macOS/lib"; do
            [ -f "$dir/$lib" ] && { found="$dir/$lib"; break; }
        done
        [ -n "$found" ] || die "--vulkan-sdk: $lib not found under $sdk/lib or $sdk/macOS/lib"
        echo "    + $lib  <- $found (Vulkan)"
        cp -L "$found" "$FW/$lib"
        chmod u+w "$FW/$lib"
        install_name_tool -id "@rpath/$lib" "$FW/$lib" 2>/dev/null
        bundle_deps "$FW/$lib" "$(dirname "$found")"
        fix_rpaths "$FW/$lib" "@loader_path"
    done
    for manifest in "$sdk/share/vulkan/icd.d/MoltenVK_icd.json" "$sdk/macOS/share/vulkan/icd.d/MoltenVK_icd.json" \
                    "$sdk/etc/vulkan/icd.d/MoltenVK_icd.json"; do
        if [ -f "$manifest" ]; then
            api="$(awk -F'"' '/"api_version"/ {print $4; exit}' "$manifest")"
            break
        fi
    done
    mkdir -p "$RES/vulkan/icd.d"
    # library_path is relative to the manifest: Contents/Resources/vulkan/icd.d -> Contents/Frameworks
    cat > "$RES/vulkan/icd.d/MoltenVK_icd.json" <<JSON
{
    "file_format_version": "1.0.0",
    "ICD": {
        "library_path": "../../../Frameworks/libMoltenVK.dylib",
        "api_version": "${api:-1.2.0}",
        "is_portability_driver": true
    }
}
JSON
}

echo "==> Bundling dylibs"
bundle_deps "$EXE" "$SRC_APP/Contents/MacOS"
fix_rpaths "$EXE" "@executable_path/../Frameworks"
for lib in "$MACOS"/*.dylib; do
    [ -f "$lib" ] || continue
    bundle_deps "$lib" "$SRC_APP/Contents/MacOS"
    fix_rpaths "$lib" "@loader_path/../Frameworks"
done
[ -z "$VULKAN_SDK_DIR" ] || bundle_vulkan "$VULKAN_SDK_DIR"

# ---------------------------------------------------------------------------
# Sign + verify
# ---------------------------------------------------------------------------

echo "==> Signing (ad-hoc)"
for lib in "$FW"/*.dylib "$MACOS"/*.dylib; do
    [ -f "$lib" ] && codesign --force --sign - --timestamp=none "$lib"
done
codesign --force --sign - --timestamp=none "$APP"
codesign --verify --strict --deep "$APP"

echo "==> Verifying"
FAIL=0
fail() { echo "  FAIL: $*" >&2; FAIL=1; }
[ -z "$(find "$APP" -type l)" ] || fail "symlinks remain: $(find "$APP" -type l | awk 'NR<=5')"
while IFS= read -r f; do
    file -b "$f" | awk '/Mach-O/ {found=1} END {exit !found}' || continue
    rel="${f#"$APP"/}"
    have=" $(lipo -archs "$f") "
    for a in $WANT_ARCHS; do
        case "$have" in *" $a "*) ;; *) fail "$rel lacks $a (has:$have)" ;; esac
    done
    for dep in $(deps_of "$f"); do
        case "$dep" in @rpath/*|@executable_path/*|@loader_path/*|/usr/lib/*|/System/*) ;; *) fail "$rel -> $dep" ;; esac
    done
    for rp in $(rpaths_of "$f"); do
        case "$rp" in @*) ;; *) fail "$rel has LC_RPATH $rp" ;; esac
    done
    echo "  ok  $rel [$(lipo -archs "$f")]"
done < <(find "$APP/Contents" -type f \( -path '*/MacOS/*' -o -path '*/Frameworks/*' -o -name '*.dylib' -o -name '*.so' \))
[ "$FAIL" = 0 ] || die "bundle verification failed"

# ---------------------------------------------------------------------------
# Archives
# ---------------------------------------------------------------------------

BASE="3SX-$SHA-macos-$ARCH"
echo "==> Archiving $BASE.{zip,dmg}"
rm -f "$OUT_DIR/$BASE.zip" "$OUT_DIR/$BASE.dmg"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$OUT_DIR/$BASE.zip"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/3sx.app"
ln -s /Applications "$STAGE/Applications"
# hdiutil occasionally fails with "Resource busy" on CI runners; retry a few times.
for attempt in 1 2 3; do
    hdiutil create -quiet -volname "3SX" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$OUT_DIR/$BASE.dmg" && break
    [ "$attempt" = 3 ] && die "hdiutil create failed"
    sleep 5
done

echo "packaged: $APP"
echo "          $OUT_DIR/$BASE.zip"
echo "          $OUT_DIR/$BASE.dmg"
