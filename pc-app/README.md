# PC app

Rust + Tauri 2. See [docs/software-plan.md](../docs/software-plan.md) for the architecture.

| Crate | Contents |
|---|---|
| `crates/mixer-core` | Channels, app rules, mixer logic, config, host thread. Pure Rust, tested without audio. |
| `crates/mixer-wasapi` | Windows Core Audio backend (`windows` crate). |
| `crates/mixer-devices` | Device protocol and hub: knobs, buttons and touch become commands; mixer state becomes screen/LED messages. See [protocol/README.md](../protocol/README.md). |
| `app/src-tauri` | The Tauri app: commands, events, tray, autostart, simulator link. |
| `app/src` | Mixer window (`main.ts`) and hardware simulator (`simulator.ts`), vanilla TypeScript. |

## Usage

- **Apps:** shown as chips with an icon in each channel. Drag a chip to another channel, or to "Not controlled". Clicking a chip gives the same choices as a menu, and Enter or arrow keys work too. "auto" means the rules placed the app. Dragging it or choosing a channel locks it there; "Automatic (rules)" unlocks it.
- **Output per channel:** the picker at the top of each channel card. Windows remembers the choice per program. If it ever needs undoing without this app: Settings → System → Sound → Volume mixer → "Reset".
- **Language:** English or Norsk, in the picker at the top. The first start follows the Windows language. The tray menu and the screen switch too, and channels that still have their default name are renamed (Game/Spill).
- **Tray:** the icon has a menu with Open mixer, Simulator, Start with Windows and Quit. A left click opens the mixer.
- **Closing the window** only hides it; the app keeps running in the tray. Quit from the tray menu.
- **Start with Windows** launches the app hidden in the tray (`--minimized`). Note: in `tauri dev`, the debug build's path is registered, so turn it off again after testing. Turn it on for real in the installed version.
- **Simulator** (button in the window, or the tray menu): a virtual device with the screen, 4 knobs and mute buttons. It speaks the same protocol as the hardware, and the log at the bottom shows every message. Turn a knob with the mouse wheel or by dragging up/down; click a knob or the screen to mute.

## Requirements

Rust (stable, MSVC), Node.js and WebView2 (already part of Windows 11).

## Commands

Run from `pc-app/app`:

```bash
npm install
npm run tauri dev
```

Run from `pc-app`:

```bash
cargo test --workspace
cargo run -p mixer-wasapi --example scan
cargo run -p mixer-wasapi --example meters
```

`meters` prints live levels per app for a few seconds and is just as read-only.

```bash
cargo run -p mixer-wasapi --example routing
cargo run -p mixer-wasapi --example icon -- <exe> <out.png>
```

`routing` checks per-app output routing on this machine without touching any real app. It routes only its own process, reads the setting back and resets it. `icon` saves an app's icon as a PNG.

The `scan` example only reads: it lists audio sessions, which channel the rules put them in, and the microphones. An optional argument selects a mic by part of its name, e.g. `scan "A50 Mic"`.

## Config

Saved to `%APPDATA%\no.diysoundmixer.app\config.json` one second after a change, and when the app exits.
