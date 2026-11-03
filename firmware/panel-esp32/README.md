# Panel firmware (CrowPanel 5" ESP32-S3)

The mixer screen, 800×480: solid dark cards on a pure black background. It is written with LVGL 9 and speaks [protocol v1](../../protocol/README.md).

| Folder | Contents |
|---|---|
| `src/` | Shared code. `mixer_ui.*` is the screen (pure LVGL); `panel_protocol.*` turns protocol lines into screen state and back (ArduinoJson). |
| `sim/` | Desktop build: the same screen in an SDL window, connected to the PC app over TCP (`127.0.0.1:47800`) instead of USB. |
| `src/fonts/` | Inter (SIL OFL 1.1) as LVGL fonts with Latin-1, so æ ø å work. Regenerate them with `gen-fonts.ps1`. |
| `panel-esp32.ino` | The CrowPanel build: RGB panel, GT911 touch and the USB-serial link. |
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

- **Mixer:** a ring and the volume per channel. Tap a card (or push its knob) to mute: the ring turns dim red, the number becomes a red MUTED and the card gets a red rim. Under the ring the card lists its output device and up to three apps ("+N more" beyond that). The mic card has a picker for the microphone.
- **Apps:** one column per playback channel, plus "Not controlled" with a count of its apps. **Tap** an app, then **tap "Move here"** in the column it should go to. In its own column the button offers "Automatic (rules)" instead; tap the app again to cancel. **Hold and drag** still works as well. Each column has a picker for its output device. Names are shortened to the part in parentheses ("A50 Voice"), and an unplugged device is shown in yellow.

In the simulator the mouse works as a finger: hold the button for about half a second to pick up an app.

### Test hook

When `PANEL_SIM_INPUT` points at a file, lines appended to it drive a virtual finger: `down X Y`, `move X Y` and `up`, in screen coordinates. This is useful for automated screenshots and tests. Posted window messages can't hold a press, because SDL checks the real mouse button.

## CrowPanel

The board is Elecrow's 5" ESP32-S3 IPS display, DLE06050B (ESP32-S3 N16R8, ST7282 RGB panel, GT911 touch). Its USB-C port goes to a CH340C on UART0, so `Serial` is the protocol link, at 921600 baud. The pins come from Elecrow's schematic.

Requires [arduino-cli](https://arduino.github.io/arduino-cli/) with the ESP32 core (3.x), LVGL 9.3 and ArduinoJson 7:

```bash
arduino-cli config add board_manager.additional_urls https://espressif.github.io/arduino-esp32/package_esp32_index.json
arduino-cli core update-index
arduino-cli core install esp32:esp32
arduino-cli lib install lvgl@9.3.0 ArduinoJson
```

Build, and upload when a port is given (quit the mixer app first, it holds the port):

```bash
pwsh firmware/panel-esp32/build.ps1
```

```bash
pwsh firmware/panel-esp32/build.ps1 -Port COMx
```

The script passes this folder as an include path, so LVGL finds our `lv_conf.h`. The CH340's DTR and RTS reset the board into the bootloader for the upload, and the mixer app releases both when it opens the port, so it does not reset the panel.

## Notes for the hardware build

- **Colours:** everything is flat, solid fills on black. Gradients and large shadows band visibly in 16-bit colour and are slow to redraw on the ESP32.
- **Highlights** (muted cards, drop targets, the picked app) add a 1 px outline to the 1 px border instead of widening the border, so the contents don't shift.
- **Transport:** USB serial replaces TCP. `panel_protocol` does not change.
