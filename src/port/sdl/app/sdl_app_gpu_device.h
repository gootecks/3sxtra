/**
 * @file sdl_app_gpu_device.h
 * @brief SDL_GPU driver selection (gpu-driver config key / --gpu-driver CLI)
 *        and device creation with a logged fallback chain.
 */
#ifndef SDL_APP_GPU_DEVICE_H
#define SDL_APP_GPU_DEVICE_H

#include <SDL3/SDL.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Record the --gpu-driver CLI value (validated later, when the device is created). */
void SDLAppGPU_SetDriverOverride(const char* driver);

/**
 * Create the SDL_GPU device and claim @p window for it.
 *
 * Order: requested driver (CLI, then config `gpu-driver`; `auto` resolves to the
 * platform preference) -> SDL's own automatic driver choice -> NULL. A driver
 * counts as failed if either device creation or the window claim fails. The
 * caller falls back to OpenGL on NULL. Initializes SDL_shadercross first and
 * requests every shader format shadercross can produce from SPIR-V (SPIR-V,
 * MSL, and DXIL/DXBC where available), so Metal and D3D12 work, not only Vulkan.
 */
SDL_GPUDevice* SDLAppGPU_CreateDevice(SDL_Window* window, bool debug_mode);

#ifdef __cplusplus
}
#endif

#endif
