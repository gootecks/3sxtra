# Build Guide

---

## 1. Prerequisites

All platforms need:
- **CMake** 3.24+
- **Clang** (C11 + C++17) — GCC works on Linux but Clang is recommended
- **Ninja** (recommended) or Make
- **Rust/Cargo** — required for librashader (shader support)
- **Python 3 + jinja2** — required for GLAD code generation
- **curl** — required to download stb headers

### Windows (MSYS2)

**Automated (recommended):**
Double-click `tools\1click_windows_v2.bat`. It downloads a portable MSYS2, installs everything, builds deps, and compiles the executable.

**Manual:**
1. Launch the **MinGW64** shell.
2. Install packages:
   ```bash
   pacman -S --needed $(cat tools/requirements-windows.txt)
   ```

The Windows requirements file includes: cmake, ninja, clang, zlib, rust, python-jinja, miniupnpc, and compiler headers.

### Linux (Ubuntu / Debian)

```bash
sudo apt-get update
sudo apt-get install -y $(cat tools/requirements-ubuntu.txt)
```

You also need Rust if not already installed:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### macOS

1. Install Xcode Command Line Tools:
   ```bash
   xcode-select --install
   ```
2. Install build tools (no Homebrew *libraries* are needed — every native dependency, including miniupnpc, is built into `third_party/`, and zlib comes from the macOS SDK):
   ```bash
   brew install cmake ninja shaderc
   python3 -m pip install --break-system-packages jinja2
   ```
3. Install Rust (plus both macOS targets if you want universal builds):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup target add aarch64-apple-darwin x86_64-apple-darwin
   ```

---

## 2. Building Dependencies

`build-deps.sh` clones and builds all third-party libraries:
SDL3, SDL3_mixer, SDL3_image, FreeType, Lua 5.4, RmlUi, GekkoNet, ControllerImage, librashader, GLAD, SIMDe, stb, Spout2, SDL_shadercross, and slang-shaders.

```bash
./build-deps.sh
```

On Windows this is handled automatically by `1click_windows_v2.bat` and `compile.bat`.

### macOS architectures

On macOS `build-deps.sh` reads two environment variables:

| Variable | Default | Meaning |
|----------|---------|---------|
| `TARGET_ARCH` | *(empty)* = host architecture | `universal` builds arm64 + x86_64 fat libraries; `arm64` / `x86_64` force one slice |
| `MACOSX_DEPLOYMENT_TARGET` | `13.0` | Minimum macOS version for every dependency |

```bash
TARGET_ARCH=universal ./build-deps.sh
```

Cached dependencies that lack a requested architecture are rebuilt automatically, so switching `TARGET_ARCH` is safe. librashader is built once per Rust target and merged with `lipo`.

---

## 3. Compiling the Game

### Linux / macOS

```bash
CC=clang CXX=clang++ cmake -B build -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DENABLE_TESTS=OFF
cmake --build build --parallel
cmake --install build --prefix build/application
```

> [!NOTE]
> On macOS, configure with the same architecture as the dependencies, e.g. `-DCMAKE_OSX_ARCHITECTURES="arm64;x86_64"` for a universal build. The deployment target defaults to 13.0.

### Windows

From a MinGW64 shell, run:
```
.\compile.bat
```

This configures with `RelWithDebInfo` by default (includes Tracy profiling). Pass `--debug` for a Debug build.

Or manually:
```bash
CC=clang CXX=clang++ cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DENABLE_TESTS=OFF
cmake --build build --parallel
cmake --install build --prefix build/application
```

### Unit Tests

```bash
cmake -B build_tests -G Ninja -DCMAKE_BUILD_TYPE=Debug -DENABLE_TESTS=ON
cmake --build build_tests --parallel
cd build_tests && ctest --output-on-failure
```

---

## 4. Cross-Compilation & Packaging

### Raspberry Pi 4 (Batocera)
Cross-compile for RPi4 using the Batocera Linux buildroot toolchain.
- Setup scripts: `tools/batocera/rpi4/`
- See `build_rpi4.yml` workflow for Docker container setup, `build-deps_rpi4.sh`, and ARM64 compilation steps.

### Flatpak
- Manifest and metadata: `flatpak/`
- Build locally with `flatpak-builder`. See `build_flatpak.yml` workflow for reference.

### macOS app bundle (universal)

`tools/macos/package-engine.sh` configures, builds and packages a self-contained `3sx.app`:

```bash
TARGET_ARCH=universal ./build-deps.sh
tools/macos/package-engine.sh --arch universal          # → dist/macos/
```

Options: `--arch universal|arm64|x86_64` (default `universal`), `--build-dir DIR` (default `build-macos-<arch>`), `--out DIR` (default `dist/macos`), `--build-type TYPE` (default `Release`), `--vulkan-sdk DIR` (optional, see below). Extra CMake arguments can be passed through `PACKAGE_CMAKE_ARGS`.

GLAD is generated at build time and needs `jinja2` in the Python CMake finds. If that interpreter lacks it, point CMake at one that has it, e.g. `uv venv /tmp/glad-venv && uv pip install --python /tmp/glad-venv/bin/python jinja2`, then `PACKAGE_CMAKE_ARGS="-DPython_EXECUTABLE=/tmp/glad-venv/bin/python -DPython3_EXECUTABLE=/tmp/glad-venv/bin/python"`.

Output:
- `3sx.app` — SDL3, SDL3_net/image/mixer and spirv-cross dylibs are copied into `Contents/Frameworks` and referenced via `@rpath`; assets, shaders and Lua scripts are real files in `Contents/Resources`; `Contents/Resources/ENGINE_VERSION` holds the UTC build time and git short SHA. The script fails if any Mach-O still references a non-system absolute path or misses a requested architecture.
- `3SX-<sha>-macos-<arch>.zip` (`ditto`) and `3SX-<sha>-macos-<arch>.dmg` (app + `/Applications` link).

When HEAD carries a `vX.Y.Z` tag, `CFBundleShortVersionString` is set to `X.Y.Z` (otherwise `0.0.0`).

`--vulkan-sdk DIR` additionally bundles the Vulkan loader (`libvulkan.1.dylib`), `libMoltenVK.dylib` and an ICD manifest at `Contents/Resources/vulkan/icd.d/MoltenVK_icd.json`. Use the LunarG Vulkan SDK (`.../macOS`), which ships universal binaries; Homebrew's `molten-vk`/`vulkan-loader` are arm64-only and fail the universal check.

The bundle is only ad-hoc signed — it is not Developer ID signed or notarised. Downloaded copies are quarantined by Gatekeeper; right-click → **Open** on first launch, or run `xattr -dr com.apple.quarantine 3sx.app`.

CI (`build_macos.yml`) runs exactly these two steps with `TARGET_ARCH=universal` and uploads `3SX-<sha>-macos-universal.{dmg,zip}` for releases.
