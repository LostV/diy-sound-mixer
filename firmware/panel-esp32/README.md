# Panel firmware (CrowPanel 5" ESP32-S3)

The mixer screen, 800×480, in design A2 (edge-lit glass, see [docs/design](../../docs/design/)). It is written with LVGL 9 and speaks [protocol v1](../../protocol/README.md).

| Folder | Contents |
|---|---|
| `src/` | Shared code. `mixer_ui.*` is the screen (pure LVGL); `panel_protocol.*` turns protocol lines into screen state and back (ArduinoJson). |
| `sim/` | Desktop build: the same screen in an SDL window, connected to the PC app over TCP (`127.0.0.1:47800`) instead of USB. |
| `fonts/` | Inter (SIL OFL 1.1) as LVGL fonts with Latin-1, so æ ø å work. Regenerate them with `gen-fonts.ps1`. |
| `lv_conf.h` | LVGL settings shared by the simulator and the hardware. |

## Simulator

Requires CMake and Visual Studio 2022 (C++). The first configure fetches SDL2, LVGL and ArduinoJson, which takes a couple of minutes.

```bash
cmake -S firmware/panel-esp32 -B firmware/panel-esp32/build-sim -G "Visual Studio 17 2022" -A x64
cmake --build firmware/panel-esp32/build-sim --config Release --target panel_sim
firmware/panel-esp32/build-sim/Release/panel_sim.exe
```

Start the PC app first (or afterwards: the simulator reconnects every second). Tap a channel to toggle mute. To connect elsewhere, pass a host and port. A dev build of the app (`npm run tauri dev`) listens on 47801 so it can run next to the installed app: `panel_sim.exe 127.0.0.1 47801`.

### The two pages

- **Mixer:** rings, volume and mute per channel. The mic card has a picker for the microphone.
- **Apps:** one column per playback channel, plus "Not controlled". **Hold** an app (long press), **drag** it to another column and **let go**. The target column lights up in its colour. A **short tap** opens a menu with the same choices, plus "Automatic (rules)". Each column has a picker for its output device. Names are shortened to the part in parentheses ("A50 Voice"), and an unplugged device is shown in yellow.

In the simulator the mouse works as a finger: hold the button for about half a second to pick up an app.

### Test hook

When `PANEL_SIM_INPUT` points at a file, lines appended to it drive a virtual finger: `down X Y`, `move X Y` and `up`, in screen coordinates. This is useful for automated screenshots and tests. Posted window messages can't hold a press, because SDL checks the real mouse button.

## Notes for the hardware build (phase 4)

- **Background:** it is computed once at startup into an RGB565 image with ordered dithering. LVGL shadows and gradients band visibly in 16-bit colour, and redrawing them every frame is slow on the ESP32. Allocation uses `malloc` (`LV_STDLIB_CLIB`), so the 768 KB buffer lands in PSRAM.
- **Cards** use flat translucency for the same reason. The edge light is a brighter top-left border.
- **Transport:** USB serial replaces TCP. `panel_protocol` does not change.
