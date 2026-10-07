# 3sxw compared with 3sxtra (macOS view)

[guimaraf/3sxw](https://github.com/guimaraf/3sxw) is a hard fork of
[crowded-street/3sx](https://github.com/crowded-street/3sx). It split from
upstream at `d5c28dd` (2026-03-13). As of 2026-10-06 it is 52 commits ahead and
176 behind, and its last commit is `1dd8515` (2026-08-20).

## Why it builds with less trouble

| Area | 3sxw | 3sxtra |
| --- | --- | --- |
| Dependencies | 5 tarballs (FFmpeg 8.0 stripped to ADX/MJPEG, SDL3 3.4.0, libcdio, minizip-ng, TF-PSA-Crypto) plus system zlib | ~17: SDL3 family, GekkoNet, SDL_net, librashader (Rust), slang-shaders, RmlUi, SDL_shadercross, FreeType, Lua, glad, ... |
| Renderer | `SDL_Renderer` only (Metal on macOS via SDL) | OpenGL 4.1 core, SDL_GPU, SDL_Renderer |
| Shader toolchain | none | SPIR-V (glslc) plus runtime SDL_shadercross |
| Netplay | removed | GekkoNet + SDL_net |
| macOS architectures | host only (arm64 on `macos-latest`) | universal (arm64 + x86_64), see [building.md](building.md) |

With no OpenGL path, 3sxw never hit the problems 3sxtra had on macOS:
- the legacy 2.1 GL context crash (GL attributes set before `SDL_Init`);
- the missing `glTexStorage*` crash;
- the Retina viewport rendering into a quarter of the window.

## Ideas taken from 3sxw

- **Portable layout.** 3sxw sets `SDL_FILESYSTEM_BASE_DIR_TYPE=parent` in
  `Info.plist`, so the ROM lives in `<folder containing 3SX.app>/resources/`. We
  did not copy this: a bundle that writes beside itself breaks under App
  Translocation and inside a read-only DMG. Instead, the 3SXtra launcher
  *probes* that location and imports the ROM into the standard preference
  folder ([macos.md](macos.md)).
- **ROM import from an ISO** (libcdio, `/THIRD/SF33RD.AFS;1` then
  `/SF33RD.AFS;1`) is the same flow upstream uses. 3sxtra accepts the extracted
  `SF33RD.AFS` file, either on its own or inside a `THIRD/` folder.

## Upstream finding that matters more

Upstream 3sx now tries SDL_GPU first (SPIR-V or MSL, with precompiled MSL
shaders) and falls back to OpenGL 3.3 core. 3sxtra's SDL_GPU path instead
transpiles SPIR-V at runtime with SDL_shadercross. The driver selection and
fallback chain are covered in [render-drivers.md](render-drivers.md).

## ROM compatibility

The file that works is `SF33RD.AFS` from the PS2 *Street Fighter Anniversary
Collection* (USA). It sits in the `THIRD/` folder of the disc, and upstream
release builds pin SHA-256
`f9fa50f3a124ec9fa9465aa9c8546c2d867887eb39f711a070762a0324ba5604`.

The Japanese standalone PS2 disc has a root-level `SF33RD.AFS` that has been
reported to work in derivatives, but it does not match that hash.

These sources do not work:
- Dreamcast;
- Xbox;
- the Steam 30th Anniversary Collection;
- CPS3 arcade sets on their own. The arcade zips (`sfiii3nr1.zip`) are
  optional extras only.
