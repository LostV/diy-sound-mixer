# Device protocol v1

The protocol between the PC app and the mixer hardware: the Pico 2 (knobs), the CrowPanel (screen) and the in-app simulator. The reference implementation is [`pc-app/crates/mixer-devices`](../pc-app/crates/mixer-devices/src). The simulator speaks the protocol exactly the way the hardware will.

## Transport

- USB CDC serial. The baud rate does not matter for USB CDC; use 115200.
- **One JSON object per line**, terminated by `\n`. `\r\n` is accepted.
- Each message has a type field `t`.
- Lines that cannot be parsed are logged and ignored on both sides. Never crash on garbage.
- The PC is the brain. Devices report input and draw what they are told, but decide nothing themselves.

## Connection

1. The PC opens the port and sends `{"t":"hello?"}`.
2. The device answers with `hello`. It should also send `hello` on its own at boot.
3. The PC then sends the full state (all `ch` and `led`). After that it only sends changes, plus `meter`.
4. A new `hello` from the device (after a reboot, for example) means "send me everything again".

## Device → PC

| Message | Meaning |
|---|---|
| `{"t":"hello","dev":"knobs","fw":"0.1.0","proto":1}` | Identification. `dev` is `knobs`, `panel` or `sim`. `proto` defaults to 1. |
| `{"t":"enc","ch":2,"d":1}` | Encoder `ch` (0–3) turned `d` detents. Positive is clockwise. Several detents may be sent as one message. |
| `{"t":"btn","id":"enc2","s":1}` | Button pressed (`s:1`) or released (`s:0`). Ids: `enc0`–`enc3` (encoder push) and `mute0`–`mute3` (separate buttons). The PC acts on **press**; release is reserved for long press later. |
| `{"t":"touch","action":"mute","ch":1}` | Touch on screen column `ch`. Only `mute` exists for now. |

## PC → device

| Message | Sent to | Meaning |
|---|---|---|
| `{"t":"hello?"}` | all | Asks the device to identify itself. |
| `{"t":"ch","i":0,"name":"Spill","vol":72,"mute":false,"color":"#30A46C","sub":"Counter-Strike 2","avail":true}` | panel, sim | Full state of column `i`, sent when anything in it changes. `sub` is a short subtitle (apps, or the mic name). `avail:false` means there is nothing to control, e.g. no microphone. |
| `{"t":"meter","v":[80,55,0,35]}` | panel, sim | Meter level per column, 0–100, **already on a dB scale** (−60..0 dB). It can be drawn directly. Sent about 30 times a second. Muted channels are 0. |
| `{"t":"led","id":2,"on":true}` | knobs, sim | Mute LED for channel `id`. |

## Slots

Knob `i`, mute button `i`, LED `i` and screen column `i` all belong to **channel number `i`** in the PC app's config. The default order is Spill, Media, Chat, Mic.

## Changes

Breaking changes bump `proto`. The device reports its version in `hello`, so the PC can tell old firmware apart.
