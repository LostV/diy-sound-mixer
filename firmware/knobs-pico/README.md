# Knob firmware (Raspberry Pi Pico 2 WH)

Reads 4 rotary encoders with push, 4 mute buttons, and drives 4 mute LEDs. It speaks [protocol v1](../../protocol/README.md) over USB serial and decides nothing itself: it sends `hello`, `enc` and `btn`, and answers `hello?` and `led`.

The code is in `knobs.cpp`. `knobs-pico.ino` is only there because Arduino wants a sketch file named after the folder.

## Pins

| Channel | Encoder A | Encoder B | Encoder push | Mute button | Mute LED |
|---|---|---|---|---|---|
| 0 (Spill) | GP0 (pin 1) | GP1 (pin 2) | GP2 (pin 4) | GP12 (pin 16) | GP16 (pin 21) |
| 1 (Media) | GP3 (pin 5) | GP4 (pin 6) | GP5 (pin 7) | GP13 (pin 17) | GP17 (pin 22) |
| 2 (Chat) | GP6 (pin 9) | GP7 (pin 10) | GP8 (pin 11) | GP14 (pin 19) | GP18 (pin 24) |
| 3 (Mic) | GP9 (pin 12) | GP10 (pin 14) | GP11 (pin 15) | GP15 (pin 20) | GP19 (pin 25) |

- Encoder common pin (C, the middle one), the second push pin and the second button pin go to GND. The inputs use the internal pull-ups, so nothing goes to 3V3.
- Each LED goes GPIO → 220 Ω → LED → GND.
- GP23, GP24, GP25 and GP29 belong to the wireless chip and are not used.

Wiring for one channel: [schematic](../../docs/koblingsskjema-knotter.svg) and [breadboard layout](../../docs/breadboard-knotter.svg). Step-by-step guide (Norwegian): [docs/test-1-knotter.md](../../docs/test-1-knotter.md).

## Settings

At the top of `knobs.cpp`:

- `HALF_STEP`: set to `true` if one click is reported as two (an encoder with half as many pulses as detents).
- `REVERSE`: flips the direction of all knobs. It is `true`, because the 41017819 counted backwards on the first breadboard. Wire all knobs the same way, or swap A and B on the odd one.

## Build

Requires [arduino-cli](https://arduino.github.io/arduino-cli/) with the [Arduino-Pico](https://github.com/earlephilhower/arduino-pico) core and ArduinoJson 7:

```bash
arduino-cli config add board_manager.additional_urls https://github.com/earlephilhower/arduino-pico/releases/download/global/package_rp2040_index.json
arduino-cli core update-index
arduino-cli core install rp2040:rp2040
arduino-cli lib install ArduinoJson
arduino-cli compile --fqbn rp2040:rp2040:rpipico2w --output-dir firmware/knobs-pico/build firmware/knobs-pico
```

The result is `build/knobs-pico.ino.uf2`.

## Flash

- **First time:** hold BOOTSEL on the Pico while plugging in the USB cable. A drive called `RP2350` appears. Copy the `.uf2` file to it, and the Pico restarts with the new firmware.
- **Later:** `arduino-cli upload --fqbn rp2040:rp2040:rpipico2w -p COMx --input-dir firmware/knobs-pico/build firmware/knobs-pico` resets it into the bootloader by itself.

## Watch it

```bash
arduino-cli monitor -p COMx -c baudrate=115200
```

The Pico sends `hello` when the port opens. Each turn and press prints a line and flashes the on-board LED. Type `{"t":"led","id":0,"on":true}` and press Enter to light mute LED 0.

The port only delivers data when the PC sets DTR. arduino-cli, the Arduino IDE and the mixer app all do this.

Only one program can have the port open. The mixer app connects to the Pico by itself, so quit it before using `arduino-cli monitor` or `upload`.
