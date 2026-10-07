/**
 * @file test_librashader_gl.c
 * @brief Libretro presets on the engine's real OpenGL context.
 *
 * Creates a hidden window with the context the engine requests on this
 * platform (4.1 core on macOS) and runs presets through the GL librashader
 * bridge. Covers:
 *  - every bundled custom preset builds a filter chain
 *  - the stock preset reproduces the input frame
 *  - an inverting preset visibly changes the output frame
 *
 * Skipped when no OpenGL context can be created (headless hosts).
 */
#include <stdarg.h>
#include <stddef.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <cmocka.h>

#include <SDL3/SDL.h>
#include <glad/gl.h>

#include "port/sdl/app/sdl_app_gl_context.h"

typedef struct LibrashaderManagerGL LibrashaderManagerGL;
LibrashaderManagerGL* LibrashaderManager_Init_GL(const char* preset_path);
void LibrashaderManager_Render_GL(LibrashaderManagerGL* manager, GLuint input_texture, int input_w, int input_h,
                                  int viewport_x, int viewport_y, int viewport_w, int viewport_h);
void LibrashaderManager_Free_GL(LibrashaderManagerGL* manager);

#define FRAME_SIZE 64
#define MAX_FAILURES 64

static SDL_Window* s_window;
static SDL_GLContext s_context;
static int s_viewport_w;
static int s_viewport_h;

static int setup_gl(void** state) {
    (void)state;
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        fprintf(stderr, "SDL_Init failed: %s\n", SDL_GetError());
        return 0;
    }
    SDLAppGL_SetContextAttributes();
    s_window = SDL_CreateWindow("test_librashader_gl", FRAME_SIZE, FRAME_SIZE, SDL_WINDOW_OPENGL | SDL_WINDOW_HIDDEN);
    if (!s_window) {
        fprintf(stderr, "No OpenGL window: %s\n", SDL_GetError());
        return 0;
    }
    s_context = SDL_GL_CreateContext(s_window);
    if (!s_context) {
        fprintf(stderr, "No OpenGL context: %s\n", SDL_GetError());
        return 0;
    }
    if (!gladLoadGL((GLADloadfunc)SDL_GL_GetProcAddress)) {
        fprintf(stderr, "gladLoadGL failed\n");
        SDL_GL_DestroyContext(s_context);
        s_context = NULL;
        return 0;
    }
    SDL_GetWindowSizeInPixels(s_window, &s_viewport_w, &s_viewport_h);
    fprintf(stderr, "GL_VERSION=%s GLSL=%s\n", glGetString(GL_VERSION), glGetString(GL_SHADING_LANGUAGE_VERSION));
    return 0;
}

static int teardown_gl(void** state) {
    (void)state;
    if (s_context)
        SDL_GL_DestroyContext(s_context);
    if (s_window)
        SDL_DestroyWindow(s_window);
    SDL_Quit();
    return 0;
}

static void require_context(void) {
    if (!s_context) {
        skip();
    }
}

static GLuint make_solid_texture(uint8_t r, uint8_t g, uint8_t b) {
    static uint8_t pixels[FRAME_SIZE * FRAME_SIZE * 4];
    for (int i = 0; i < FRAME_SIZE * FRAME_SIZE; i++) {
        pixels[i * 4 + 0] = r;
        pixels[i * 4 + 1] = g;
        pixels[i * 4 + 2] = b;
        pixels[i * 4 + 3] = 255;
    }
    GLuint tex = 0;
    glGenTextures(1, &tex);
    glBindTexture(GL_TEXTURE_2D, tex);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8, FRAME_SIZE, FRAME_SIZE, 0, GL_RGBA, GL_UNSIGNED_BYTE, pixels);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
    return tex;
}

/* Renders a solid-colour frame through the preset and returns the centre pixel
   of the default framebuffer, which is cleared to magenta first so a skipped
   pass cannot pass as success. */
static void render_preset_center(const char* preset, const uint8_t in[3], uint8_t out[4]) {
    LibrashaderManagerGL* manager = LibrashaderManager_Init_GL(preset);
    if (!manager)
        fail_msg("LibrashaderManager_Init_GL failed for %s", preset);

    GLuint input = make_solid_texture(in[0], in[1], in[2]);
    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glViewport(0, 0, s_viewport_w, s_viewport_h);
    glClearColor(1.0f, 0.0f, 1.0f, 1.0f);
    glClear(GL_COLOR_BUFFER_BIT);

    LibrashaderManager_Render_GL(manager, input, FRAME_SIZE, FRAME_SIZE, 0, 0, s_viewport_w, s_viewport_h);
    glFinish();

    glBindFramebuffer(GL_READ_FRAMEBUFFER, 0);
    glPixelStorei(GL_PACK_ALIGNMENT, 1);
    glReadPixels(s_viewport_w / 2, s_viewport_h / 2, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, out);
    assert_int_equal(glGetError(), GL_NO_ERROR);

    glDeleteTextures(1, &input);
    LibrashaderManager_Free_GL(manager);
}

static void assert_rgb_near(const uint8_t actual[4], int r, int g, int b) {
    const int tolerance = 2;
    if (abs(actual[0] - r) > tolerance || abs(actual[1] - g) > tolerance || abs(actual[2] - b) > tolerance) {
        fail_msg("pixel (%d,%d,%d), expected (%d,%d,%d)", actual[0], actual[1], actual[2], r, g, b);
    }
}

/* Enumerates the presets shipped in assets/ but loads the copies inside
   slang-shaders/custom, where their ../ references resolve at runtime. */
static void check_bundled_presets(char failures[][256], int* failure_count, int* total) {
    int count = 0;
    char** entries = SDL_GlobDirectory(BUNDLED_PRESET_DIR, NULL, 0, &count);
    if (!entries)
        fail_msg("cannot list %s: %s", BUNDLED_PRESET_DIR, SDL_GetError());
    for (int i = 0; i < count; i++) {
        size_t len = strlen(entries[i]);
        if (len < 7 || strcmp(entries[i] + len - 7, ".slangp") != 0)
            continue;
        char path[1024];
        snprintf(path, sizeof(path), "%s/%s", CUSTOM_PRESET_DIR, entries[i]);
        (*total)++;
        LibrashaderManagerGL* manager = LibrashaderManager_Init_GL(path);
        if (manager) {
            LibrashaderManager_Free_GL(manager);
        } else if (*failure_count < MAX_FAILURES) {
            snprintf(failures[(*failure_count)++], 256, "%s", entries[i]);
        }
    }
    SDL_free(entries);
}

static void test_bundled_custom_presets_build_filter_chains(void** state) {
    (void)state;
    require_context();

    static char failures[MAX_FAILURES][256];
    int failure_count = 0;
    int total = 0;
    check_bundled_presets(failures, &failure_count, &total);

    assert_true(total > 0);
    for (int i = 0; i < failure_count; i++) {
        fprintf(stderr, "preset failed to build: %s\n", failures[i]);
    }
    if (failure_count > 0)
        fail_msg("%d of %d bundled custom presets failed on %s", failure_count, total, glGetString(GL_VERSION));
}

static void test_stock_preset_reproduces_input(void** state) {
    (void)state;
    require_context();

    const uint8_t in[3] = { 200, 40, 10 };
    uint8_t out[4];
    render_preset_center(CUSTOM_PRESET_DIR "/stock.slangp", in, out);
    assert_rgb_near(out, 200, 40, 10);
}

static void test_invert_preset_changes_output(void** state) {
    (void)state;
    require_context();

    const uint8_t in[3] = { 200, 40, 10 };
    uint8_t out[4];
    render_preset_center(FIXTURE_DIR "/invert.slangp", in, out);
    assert_rgb_near(out, 55, 215, 245);
}

int main(void) {
    const struct CMUnitTest tests[] = {
        cmocka_unit_test(test_bundled_custom_presets_build_filter_chains),
        cmocka_unit_test(test_stock_preset_reproduces_input),
        cmocka_unit_test(test_invert_preset_changes_output),
    };
    return cmocka_run_group_tests(tests, setup_gl, teardown_gl);
}
