// LVGL configuration shared by the CrowPanel firmware and the desktop
// simulator. Only what differs from LVGL's defaults is set here.

#ifndef LV_CONF_H
#define LV_CONF_H

// The CrowPanel's RGB panel is 16-bit.
#define LV_COLOR_DEPTH 16

#define LV_USE_OS LV_OS_NONE
#define LV_DEF_REFR_PERIOD 16

// The system allocator, not LVGL's small built-in pool: the computed
// background image alone is 768 KB. On the ESP32-S3 large blocks go to PSRAM.
#define LV_USE_STDLIB_MALLOC LV_STDLIB_CLIB
#define LV_USE_STDLIB_STRING LV_STDLIB_CLIB
#define LV_USE_STDLIB_SPRINTF LV_STDLIB_CLIB

#define LV_USE_LOG 1
#define LV_LOG_LEVEL LV_LOG_LEVEL_WARN
#define LV_LOG_PRINTF 1

// Our own fonts (fonts/) cover Norwegian letters; keep one built-in fallback.
#define LV_FONT_MONTSERRAT_14 1
#define LV_FONT_DEFAULT &lv_font_montserrat_14

#ifdef PANEL_SIMULATOR
#define LV_USE_SDL 1
#define LV_SDL_INCLUDE_PATH <SDL.h>
#define LV_SDL_RENDER_MODE LV_DISPLAY_RENDER_MODE_DIRECT
#define LV_SDL_BUF_COUNT 1
#define LV_SDL_FULLSCREEN 0
#define LV_SDL_DIRECT_EXIT 1
#endif

#endif  // LV_CONF_H
