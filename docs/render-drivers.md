# Render backends and GPU drivers

3SX has four renderer backends. The `gpu` backend runs on SDL3's `SDL_GPU`, which itself has several drivers (Metal, Vulkan, D3D12). This page covers how each is chosen, how the fallback works, and how to get Vulkan on macOS.

## Backends

| `renderer` | Backend | Notes |
|---|---|---|
| `gl` (default) | OpenGL (4.1 core on macOS, 4.6 core on desktop, GLES on Android) | Full feature set, including libretro `.slangp` shader presets |
| `gpu` | SDL_GPU (Metal / Vulkan / D3D12) | Shaders ship as SPIR-V and are translated at runtime by SDL_shadercross |
| `sdl` | SDL_Renderer (2D) | Simple path. It is also the last-resort fallback |
| `classic` | SDL_Renderer, unoptimized | Benchmark/reference path |

## Driver matrix (`renderer = gpu`)

| Platform | `auto` picks | Also selectable | Shader path |
|---|---|---|---|
| macOS | `metal` (SDL's default order) | `vulkan` (needs MoltenVK or KosmicKrisp, see below) | SPIR-V → MSL (Metal), SPIR-V (Vulkan) |
| Windows | `vulkan` | `d3d12` | SPIR-V (Vulkan), SPIR-V → DXIL/DXBC (D3D12, needs dxcompiler/d3dcompiler at runtime) |
| Linux / Raspberry Pi | `vulkan` | — | SPIR-V |

`auto` prefers Vulkan everywhere except Apple because the libretro shader-preset path (librashader) on the GPU backend only works on Vulkan. On Metal or D3D12, presets are refused with `Librashader: Unsupported GPU backend '<name>'. Only Vulkan is supported.` and the game renders without them.

The device advertises every format `SDL_ShaderCross_GetSPIRVShaderFormats()` reports (SPIR-V and MSL always, plus DXIL/DXBC when shadercross has DXC or d3dcompiler). All SPIR-V consumers go through shadercross reflection and compile: the game renderer, bezel, text renderer and the LZ77 compute pipeline. As a result, the same `.spv` files work on Metal.

## Knobs

| Where | Setting | Values |
|---|---|---|
| config file | `renderer` | `auto` \| `gl` \| `gpu` \| `sdl` \| `classic` |
| config file | `gpu-driver` | `auto` (default) \| `metal` \| `vulkan` \| `d3d12` |
| CLI | `--renderer <…>` | as above; overrides config |
| CLI | `--gpu-driver <…>` | as above; overrides config |
| env | `SDL_GPU_DRIVER` | SDL hint. When set, it **overrides** `gpu-driver`, and the game logs a warning |
| env | `SDL_VULKAN_LIBRARY` | Path of the Vulkan loader or ICD dylib that SDL should load |
| env | `VK_DRIVER_FILES` (legacy `VK_ICD_FILENAMES`) | ICD JSON(s) for the Khronos loader |
| env | `SDL_GPU_DEBUG=1` | Enables driver validation/debug mode |

An invalid value is never ignored silently. An unknown `gpu-driver` logs `[GPU] Invalid gpu-driver '<v>' (from <source>) … Using auto.` An unknown `--renderer` logs `[CLI] Unknown --renderer …; using gl`. An unknown config `renderer` logs `Invalid config renderer …; using platform default`.

## Fallback order

1. The requested SDL_GPU driver: `--gpu-driver`, otherwise config `gpu-driver`, where `auto` means the platform preference above.
2. SDL_GPU automatic selection (SDL's order is Metal → D3D12 → Vulkan).
3. OpenGL. The window is recreated with an OpenGL context.
4. SDL_Renderer (`sdl`), used if the OpenGL context cannot be created.

A driver counts as failed if either device creation or the window claim fails.

## macOS default: `gl`

Evidence gathered on an M4 running macOS 27.2 with arm64 Release builds:

- `--renderer gpu --gpu-driver metal` boots to the title (`[BOOT] Loop_Demo: CAPCOM_Logo done -> Title`). It logs no shader errors. The LZ77 compute pipeline, bezel and text renderer all initialize.
- `--renderer gpu --gpu-driver vulkan` with Homebrew MoltenVK also boots to the title.

The default nevertheless stays `gl`. The GPU backend is not feature-equivalent on Metal: libretro shader presets are Vulkan-only on SDL_GPU, and modded stages plus librashader are unsupported on the GPU backend everywhere. OpenGL 4.1 is deprecated on macOS but still works and supports both features. Use `renderer = gpu` if you don't need presets.

## Vulkan on macOS

SDL builds its Vulkan driver on Apple and handles `VK_KHR_portability_enumeration` and `VK_KHR_portability_subset` itself. All it needs is a Vulkan implementation it can load. It searches `@executable_path/../Frameworks/libMoltenVK.dylib`, then `vulkan.framework`, `libvulkan.1.dylib`, `libvulkan.dylib`, `MoltenVK.framework` and `libMoltenVK.dylib`. Homebrew's `/opt/homebrew/lib` is not on the default dlopen path, so a Homebrew install needs the environment variables below.

### MoltenVK from Homebrew (verified)

```sh
brew install molten-vk vulkan-loader
SDL_VULKAN_LIBRARY=/opt/homebrew/lib/libvulkan.1.dylib \
VK_DRIVER_FILES=/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json \
  3sx.app/Contents/MacOS/3sx --renderer gpu --gpu-driver vulkan
```

Expected log: `[GPU] SDL_GPU device created: driver=vulkan (requested=vulkan)`.

### Bundled MoltenVK

Put `libMoltenVK.dylib` at `3sx.app/Contents/Frameworks/libMoltenVK.dylib`. SDL loads it directly (no loader, no ICD JSON), so `--gpu-driver vulkan` then works without any environment variables. For a universal app the dylib must be universal. The LunarG Vulkan SDK ships one.

### LunarG Vulkan SDK

Install the SDK and source its `setup-env.sh`, or point `SDL_VULKAN_LIBRARY` at the SDK's `libvulkan.1.dylib` and `VK_DRIVER_FILES` at its `MoltenVK_icd.json`.

### KosmicKrisp (Apple Silicon, macOS 26+)

[KosmicKrisp](https://docs.mesa3d.org/drivers/kosmickrisp.html) is LunarG's Mesa Vulkan 1.4 driver on Metal 4. Use it through the Khronos loader with its ICD JSON:

```sh
SDL_VULKAN_LIBRARY=/opt/homebrew/lib/libvulkan.1.dylib \
VK_DRIVER_FILES=/path/to/kosmickrisp_icd.json \
  3sx --renderer gpu --gpu-driver vulkan
```

It is arm64-only and requires macOS 26 or later, so it cannot be the fallback for a universal build.

### Highball's MoltenVK (not recommended)

Highball, the Wine-based launcher, installs a patched MoltenVK 1.4.1 at `~/Library/Application Support/Highball/engines/<engine>/frameworks/libMoltenVK.dylib`. It ships no ICD JSON. To try it, write one:

```json
{"file_format_version":"1.0.0","ICD":{"library_path":"/Users/<you>/Library/Application Support/Highball/engines/<engine>/frameworks/libMoltenVK.dylib","api_version":"1.4.0","is_portability_driver":true}}
```

Then set `VK_DRIVER_FILES` to that JSON, or set `SDL_VULKAN_LIBRARY` to the dylib directly. Avoid it, for three reasons:

- The dylib is **x86_64 only**, so an arm64 or universal 3SX cannot load it. Only the x86_64 slice under Rosetta can.
- Its only change is a Wine-specific patch (`MVK_SHADOW_IMPORT`) on stock 1.4.1.
- Homebrew or bundled MoltenVK provides the same driver natively.

## Troubleshooting log lines

| Log line | Meaning |
|---|---|
| `[GPU] SDL_GPU drivers compiled in: metal vulkan` | Drivers this SDL build has |
| `[GPU] Shader formats offered …: SPIRV MSL` | Formats shadercross can produce from SPIR-V |
| `[GPU] Requested driver: <x>` | Result of CLI/config resolution |
| `[GPU] SDL_GPU driver vulkan failed: SDL_HINT_GPU_DRIVER vulkan unsupported!` | SDL could not load a Vulkan implementation. It is followed by a hint about MoltenVK/`SDL_VULKAN_LIBRARY` on macOS |
| `[GPU] Falling back to SDL_GPU automatic driver selection` | Step 2 of the chain |
| `[GPU] SDL_GPU device created: driver=<d> (requested=<r>)` | Final driver; a warning follows if `d` differs from `r` |
| `[GPU] SDL_GPU_DRIVER=<v> is set in the environment and overrides gpu-driver` | Environment hint in effect |
| `[GPU] No SDL_GPU driver could be created — falling back to OpenGL` | Step 3 |
| `Failed to create OpenGL context: … — falling back to SDL renderer` | Step 4 |
| `Librashader: Unsupported GPU backend 'metal'` | Shader presets need Vulkan (or `--renderer gl`) |
