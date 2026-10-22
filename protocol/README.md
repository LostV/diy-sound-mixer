# Device protocol v1

The protocol between the PC app and the mixer hardware: the Pico 2 (knobs), the CrowPanel (screen) and the in-app simulator. The reference implementation is [`pc-app/crates/mixer-devices`](../pc-app/crates/mixer-devices/src). The simulator speaks the protocol exactly the way the hardware will.

## Transport

- USB CDC serial. The baud rate does not matter for USB CDC; use 115200.
- During development, also local TCP on `127.0.0.1:47800`, used by the desktop simulator of the screen. Each connection is its own device.
- **One JSON object per line**, terminated by `\n`. `\r\n` is accepted.
- Each message has a type field `t`.
- Lines that cannot be parsed are logged and ignored on both sides. Never crash on garbage.
- The PC is the brain. Devices report input and draw what they are told, but decide nothing themselves.

## Connection

1. The PC opens the port and sends `{"t":"hello?"}`.
2. The device answers with `hello`. It should also send `hello` on its own at boot.
3. The PC then sends the full state (`lang`, all `ch` and `led`). After that it only sends changes, plus `meter`.
4. A new `hello` from the device (after a reboot, for example) means "send me everything again".

## Device → PC

| Message | Meaning |
|---|---|
| `{"t":"hello","dev":"knobs","fw":"0.1.0","proto":1}` | Identification. `dev` is `knobs`, `panel` or `sim`. `proto` defaults to 1. |
| `{"t":"enc","ch":2,"d":1}` | Encoder `ch` (0–3) turned `d` detents. Positive is clockwise. Several detents may be sent as one message. |
| `{"t":"btn","id":"enc2","s":1}` | Button pressed (`s:1`) or released (`s:0`). Ids: `enc0`–`enc3` (encoder push) and `mute0`–`mute3` (separate buttons). The PC acts on **press**; release is reserved for long press later. |
| `{"t":"touch","action":"mute","ch":1}` | Touch on screen column `ch`. Only `mute` exists for now. |
| `{"t":"assign","app":"Discord.exe","to":2}` | The user moved an app on the screen's app page. `to` is a channel slot, `"none"` (leave the app alone) or `"auto"` (let the rules decide again). Slots of the mic channel are ignored. |
| `{"t":"device","ch":1,"name":"Headset Earphone (A50 Voice)"}` | The user picked a device for slot `ch`: the output of a playback channel, or the microphone of the mic channel. `""` means the Windows default. |

## PC → device

| Message | Sent to | Meaning |
|---|---|---|
| `{"t":"hello?"}` | all | Asks the device to identify itself. |
| `{"t":"ch","i":0,"name":"Spill","vol":72,"mute":false,"color":"#30A46C","sub":"Counter-Strike 2","avail":true,"cap":false}` | panel, sim | Full state of column `i`, sent when anything in it changes. `sub` is a short subtitle (apps, or the mic name). `avail:false` means there is nothing to control, e.g. no microphone. `cap:true` marks the microphone channel, which takes no apps (missing = false). |
| `{"t":"meter","v":[80,55,0,35]}` | panel, sim | Meter level per column, 0–100, **already on a dB scale** (−60..0 dB). It can be drawn directly. Sent about 30 times a second. Muted channels are 0. |
| `{"t":"led","id":2,"on":true}` | knobs, sim | Mute LED for channel `id`. |
| `{"t":"lang","v":"no"}` | panel, sim | Language for the device's own texts (`en` or `no`): mute labels, connection status and so on. Sent before the channels and again when the user switches. Until it arrives, devices use English. Channel names come translated in `ch`. |

### App page (panel only)

The CrowPanel has a touch screen with an app page where apps are dragged between channels. Only `dev:"panel"` gets these messages; they are sent after the channels, and again only when they change.

| Message | Meaning |
|---|---|
| `{"t":"icon","id":"Discord.exe","w":24,"h":24,"px":"…"}` | An app icon, sent once per app and connection, before the `apps` that uses it. `px` is base64 of LVGL's RGB565A8 layout: `w*h` little-endian RGB565 pixels, then `w*h` alpha bytes. |
| `{"t":"apps","list":[{"id":"Discord.exe","name":"Discord","ch":2,"auto":true,"icon":true}]}` | The apps that play sound, one entry per program. `ch` is the channel slot, or `-1` when not controlled. `auto` marks apps placed by the rules rather than by the user. `icon` says an `icon` was sent. |
| `{"t":"devices","out":["Headphones (A50 Game)",…],"in":["Headset Microphone (A50 Mic)",…],"sel":["","Soundbar","","Headset Microphone (A50 Mic)"],"routing":true}` | Choices for the device pickers. `sel[i]` is slot i's current choice: an output for playback channels, a microphone for the mic channel. `""` is the Windows default. A choice missing from the lists is unplugged. `routing:false` means Windows refuses per-app outputs, so output pickers are hidden. |

## Slots

Knob `i`, mute button `i`, LED `i` and screen column `i` all belong to **channel number `i`** in the PC app's config. The default order is Spill, Media, Chat, Mic.

## Changes

Breaking changes bump `proto`. The device reports its version in `hello`, so the PC can tell old firmware apart.
