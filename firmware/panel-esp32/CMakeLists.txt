# Desktop simulator of the panel firmware (LVGL + SDL2).
#
#   cmake -S . -B build-sim
#   cmake --build build-sim --config Release
#   build-sim/Release/panel_sim.exe
#
# The CrowPanel build itself uses PlatformIO with the same src/ and fonts/.

cmake_minimum_required(VERSION 3.20)
project(panel_sim C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(MSVC)
  add_compile_options(/utf-8)
endif()

include(FetchContent)
set(FETCHCONTENT_QUIET ON)

# SDL2, linked statically so the simulator is a single exe.
set(SDL_SHARED OFF CACHE BOOL "" FORCE)
set(SDL_STATIC ON CACHE BOOL "" FORCE)
set(SDL_TEST OFF CACHE BOOL "" FORCE)
FetchContent_Declare(SDL2
  GIT_REPOSITORY https://github.com/libsdl-org/SDL.git
  GIT_TAG release-2.30.9
  GIT_SHALLOW TRUE)
FetchContent_MakeAvailable(SDL2)

# LVGL, configured by our lv_conf.h.
set(LV_CONF_PATH ${CMAKE_CURRENT_SOURCE_DIR}/lv_conf.h CACHE STRING "" FORCE)
set(LV_CONF_BUILD_DISABLE_EXAMPLES ON CACHE BOOL "" FORCE)
set(LV_CONF_BUILD_DISABLE_DEMOS ON CACHE BOOL "" FORCE)
set(LV_CONF_BUILD_DISABLE_THORVG_INTERNAL ON CACHE BOOL "" FORCE)
FetchContent_Declare(lvgl
  GIT_REPOSITORY https://github.com/lvgl/lvgl.git
  GIT_TAG v9.3.0
  GIT_SHALLOW TRUE)
FetchContent_MakeAvailable(lvgl)
target_compile_definitions(lvgl PUBLIC PANEL_SIMULATOR LV_CONF_INCLUDE_SIMPLE)
target_include_directories(lvgl PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(lvgl PUBLIC SDL2::SDL2-static)

# ArduinoJson is header only and works on the desktop too.
FetchContent_Declare(ArduinoJson
  GIT_REPOSITORY https://github.com/bblanchon/ArduinoJson.git
  GIT_TAG v7.2.1
  GIT_SHALLOW TRUE)
FetchContent_MakeAvailable(ArduinoJson)

file(GLOB PANEL_FONTS ${CMAKE_CURRENT_SOURCE_DIR}/fonts/*.c)
add_executable(panel_sim
  sim/main.cpp
  src/mixer_ui.cpp
  src/panel_protocol.cpp
  ${PANEL_FONTS})
target_link_libraries(panel_sim PRIVATE lvgl ArduinoJson SDL2::SDL2-static)
if(WIN32)
  target_link_libraries(panel_sim PRIVATE ws2_32)
endif()
