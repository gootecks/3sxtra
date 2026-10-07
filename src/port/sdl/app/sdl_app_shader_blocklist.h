#ifndef SDL_APP_SHADER_BLOCKLIST_H
#define SDL_APP_SHADER_BLOCKLIST_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

bool SDLAppShader_IsPresetBlocked(const char* preset_rel_path);

#ifdef __cplusplus
}
#endif

#endif /* SDL_APP_SHADER_BLOCKLIST_H */
