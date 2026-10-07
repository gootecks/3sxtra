/**
 * @file sdl_app_gpu_device.c
 * @brief SDL_GPU driver selection and device creation.
 *
 * All GPU shaders ship as SPIR-V and are translated at runtime by
 * SDL_shadercross (SPIR-V -> MSL on Metal, -> DXIL/DXBC on D3D12, passthrough on
 * Vulkan). The device therefore advertises every format shadercross can emit.
 *
 * Fallback chain (each step logged with a "[GPU]" prefix):
 *   1. requested driver (--gpu-driver, else config `gpu-driver`; `auto` maps to
 *      the platform preference: SDL's default (Metal) on Apple, Vulkan elsewhere
 *      because the librashader GPU path is Vulkan-only)
 *   2. SDL's automatic driver choice
 *   3. NULL -> caller falls back to OpenGL (and from there to SDL_Renderer)
 *
 * See docs/render-drivers.md.
 */
#include "port/sdl/app/sdl_app_gpu_device.h"
#include "port/config/config.h"

#include <SDL3_shadercross/SDL_shadercross.h>

static const char* s_cli_driver = NULL;

void SDLAppGPU_SetDriverOverride(const char* driver) {
    s_cli_driver = driver;
}

/** Map a user value to a canonical SDL GPU driver name, or NULL for auto. Logs invalid values. */
static const char* normalize_driver(const char* value, const char* source) {
    static const char* const valid[] = { "metal", "vulkan", "d3d12" };

    if (value == NULL || value[0] == '\0' || SDL_strcasecmp(value, "auto") == 0) {
        return NULL;
    }
    for (size_t i = 0; i < SDL_arraysize(valid); i++) {
        if (SDL_strcasecmp(value, valid[i]) == 0) {
            return valid[i];
        }
    }
    SDL_LogWarn(SDL_LOG_CATEGORY_APPLICATION,
                "[GPU] Invalid gpu-driver '%s' (from %s); valid values: auto|metal|vulkan|d3d12. Using auto.",
                value,
                source);
    return NULL;
}

static const char* platform_preferred_driver(void) {
#ifdef __APPLE__
    return NULL; // SDL tries Metal first on Apple
#else
    return "vulkan"; // pre-existing behaviour; librashader GPU presets need Vulkan
#endif
}

static void log_shader_formats(SDL_GPUShaderFormat formats) {
    SDL_Log("[GPU] Shader formats offered (SPIR-V via SDL_shadercross):%s%s%s%s",
            (formats & SDL_GPU_SHADERFORMAT_SPIRV) ? " SPIRV" : "",
            (formats & SDL_GPU_SHADERFORMAT_MSL) ? " MSL" : "",
            (formats & SDL_GPU_SHADERFORMAT_DXIL) ? " DXIL" : "",
            (formats & SDL_GPU_SHADERFORMAT_DXBC) ? " DXBC" : "");
}

static void log_compiled_drivers(void) {
    char list[128] = { 0 };
    const int n = SDL_GetNumGPUDrivers();
    for (int i = 0; i < n; i++) {
        SDL_strlcat(list, " ", sizeof(list));
        SDL_strlcat(list, SDL_GetGPUDriver(i), sizeof(list));
    }
    SDL_Log("[GPU] SDL_GPU drivers compiled in:%s", n > 0 ? list : " (none)");
}

/** One device creation + window claim attempt. name == NULL lets SDL choose. */
static SDL_GPUDevice* try_create(const char* name, SDL_GPUShaderFormat formats, SDL_Window* window, bool debug_mode) {
    SDL_PropertiesID props = SDL_CreateProperties();
    SDL_SetBooleanProperty(props, SDL_PROP_GPU_DEVICE_CREATE_DEBUGMODE_BOOLEAN, debug_mode);
    SDL_SetBooleanProperty(
        props, SDL_PROP_GPU_DEVICE_CREATE_SHADERS_SPIRV_BOOLEAN, (formats & SDL_GPU_SHADERFORMAT_SPIRV) != 0);
    SDL_SetBooleanProperty(
        props, SDL_PROP_GPU_DEVICE_CREATE_SHADERS_MSL_BOOLEAN, (formats & SDL_GPU_SHADERFORMAT_MSL) != 0);
    SDL_SetBooleanProperty(
        props, SDL_PROP_GPU_DEVICE_CREATE_SHADERS_DXIL_BOOLEAN, (formats & SDL_GPU_SHADERFORMAT_DXIL) != 0);
    SDL_SetBooleanProperty(
        props, SDL_PROP_GPU_DEVICE_CREATE_SHADERS_DXBC_BOOLEAN, (formats & SDL_GPU_SHADERFORMAT_DXBC) != 0);
    if (name) {
        SDL_SetStringProperty(props, SDL_PROP_GPU_DEVICE_CREATE_NAME_STRING, name);
    }

    SDL_Log("[GPU] Trying SDL_GPU driver: %s", name ? name : "auto (SDL default order)");
    SDL_GPUDevice* dev = SDL_CreateGPUDeviceWithProperties(props);

#ifdef PLATFORM_RPI4
    // V3D GPU: retry with reduced feature set if default creation failed
    if (!dev) {
        SDL_Log("[GPU] Retrying with reduced features for V3D...");
        SDL_SetBooleanProperty(props, SDL_PROP_GPU_DEVICE_CREATE_FEATURE_ANISOTROPY_BOOLEAN, false);
        SDL_SetBooleanProperty(props, SDL_PROP_GPU_DEVICE_CREATE_FEATURE_DEPTH_CLAMPING_BOOLEAN, false);
        SDL_SetBooleanProperty(props, SDL_PROP_GPU_DEVICE_CREATE_FEATURE_CLIP_DISTANCE_BOOLEAN, false);
        SDL_SetBooleanProperty(props, SDL_PROP_GPU_DEVICE_CREATE_FEATURE_INDIRECT_DRAW_FIRST_INSTANCE_BOOLEAN, false);
        // Disable 'near universal' features not supported by V3D (patched in SDL3)
        SDL_SetBooleanProperty(props, "SDL.gpu.device.create.feature.image_cube_array", false);
        SDL_SetBooleanProperty(props, "SDL.gpu.device.create.feature.independent_blend", false);
        SDL_SetBooleanProperty(props, "SDL.gpu.device.create.feature.sample_rate_shading", false);
        dev = SDL_CreateGPUDeviceWithProperties(props);
    }
#endif

    SDL_DestroyProperties(props);
    if (!dev) {
        SDL_LogWarn(SDL_LOG_CATEGORY_APPLICATION,
                    "[GPU] SDL_GPU driver %s failed: %s",
                    name ? name : "auto",
                    SDL_GetError());
#ifdef __APPLE__
        if (name && SDL_strcmp(name, "vulkan") == 0) {
            SDL_Log("[GPU] Hint: no Vulkan loader/MoltenVK found. Bundle Contents/Frameworks/libMoltenVK.dylib or set "
                    "SDL_VULKAN_LIBRARY (+ VK_DRIVER_FILES); see docs/render-drivers.md");
        }
#endif
        return NULL;
    }
    if (!SDL_ClaimWindowForGPUDevice(dev, window)) {
        SDL_LogWarn(SDL_LOG_CATEGORY_APPLICATION,
                    "[GPU] SDL_GPU driver %s created but could not claim the window: %s",
                    SDL_GetGPUDeviceDriver(dev),
                    SDL_GetError());
        SDL_DestroyGPUDevice(dev);
        return NULL;
    }
    return dev;
}

SDL_GPUDevice* SDLAppGPU_CreateDevice(SDL_Window* window, bool debug_mode) {
    const char* requested;
    if (s_cli_driver) {
        requested = normalize_driver(s_cli_driver, "--gpu-driver");
    } else {
        requested = normalize_driver(Config_GetString(CFG_KEY_GPU_DRIVER), "config gpu-driver");
    }
    const bool explicit_request = (requested != NULL);
    if (!requested) {
        requested = platform_preferred_driver();
    }

    const char* env_hint = SDL_GetHint(SDL_HINT_GPU_DRIVER);
    if (env_hint && env_hint[0] != '\0') {
        SDL_LogWarn(SDL_LOG_CATEGORY_APPLICATION,
                    "[GPU] SDL_GPU_DRIVER=%s is set in the environment and overrides gpu-driver",
                    env_hint);
    }

    SDL_ShaderCross_Init();
    const SDL_GPUShaderFormat formats = SDL_ShaderCross_GetSPIRVShaderFormats();
    log_compiled_drivers();
    log_shader_formats(formats);
    SDL_Log("[GPU] Requested driver: %s%s",
            requested ? requested : "auto",
            explicit_request ? "" : " (gpu-driver=auto, platform preference)");

    SDL_GPUDevice* dev = try_create(requested, formats, window, debug_mode);
    if (!dev && requested) {
        SDL_Log("[GPU] Falling back to SDL_GPU automatic driver selection");
        dev = try_create(NULL, formats, window, debug_mode);
    }

    if (!dev) {
        SDL_ShaderCross_Quit();
        return NULL;
    }

    const char* chosen = SDL_GetGPUDeviceDriver(dev);
    SDL_Log("[GPU] SDL_GPU device created: driver=%s (requested=%s)", chosen, requested ? requested : "auto");
    if (requested && SDL_strcmp(chosen, requested) != 0) {
        SDL_LogWarn(
            SDL_LOG_CATEGORY_APPLICATION, "[GPU] Requested driver %s unavailable; running on %s", requested, chosen);
    }
    return dev;
}
