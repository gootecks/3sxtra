/**
 * @file sdl_app_gl_context.h
 * @brief OpenGL context attributes requested by the engine on each platform.
 */
#ifndef SDL_APP_GL_CONTEXT_H
#define SDL_APP_GL_CONTEXT_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Set the SDL_GL context attributes for the platform: GLES 3.0 on Android,
 * 3.3 core on Raspberry Pi 4, 4.1 core forward-compatible on macOS and
 * 4.6 core elsewhere. Must run after SDL_Init(SDL_INIT_VIDEO) and before the
 * OpenGL window/context is created.
 */
void SDLAppGL_SetContextAttributes(void);

#ifdef __cplusplus
}
#endif

#endif
