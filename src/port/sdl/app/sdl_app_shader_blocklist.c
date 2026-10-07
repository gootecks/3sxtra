#include "port/sdl/app/sdl_app_shader_blocklist.h"

#include <SDL3/SDL.h>

#ifdef __APPLE__
static const char* const k_blocked_substrings[] = {
    "megatron",
    "scanline-classic/presets/steamdeck-oled-native/",
    "scanline-classic/presets/uhd-4k-wcg/",
    "koko-aio/Presets_HiresGames_Fast/Presets_Handhelds-ng/PSP",
    "Mega_Bezel/shaders/hyllian/crt-super-xbr/crt-super-xbr.slangp",
};

static bool contains_ci(const char* haystack, const char* needle) {
    const size_t n = SDL_strlen(needle);
    for (const char* p = haystack; *p; p++) {
        if (SDL_strncasecmp(p, needle, n) == 0) {
            return true;
        }
    }
    return false;
}
#endif

bool SDLAppShader_IsPresetBlocked(const char* preset_rel_path) {
#ifdef __APPLE__
    for (size_t i = 0; i < SDL_arraysize(k_blocked_substrings); i++) {
        if (contains_ci(preset_rel_path, k_blocked_substrings[i])) {
            return true;
        }
    }
#else
    (void)preset_rel_path;
#endif
    return false;
}
