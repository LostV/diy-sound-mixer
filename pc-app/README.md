# PC app

Rust + Tauri 2. See [docs/software-plan.md](../docs/software-plan.md) for the architecture.

| Crate | Contents |
|---|---|
| `crates/mixer-core` | Channels, app rules, mixer logic, config, host thread. Pure Rust, tested without audio. |
| `crates/mixer-wasapi` | Windows Core Audio backend (`windows` crate). |
| `app/src-tauri` | The Tauri app: commands, events, startup. |
| `app/src` | The settings/test window (vanilla TypeScript). |

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
```

The `scan` example only reads: it lists audio sessions, which channel the rules put them in, and the microphones. An optional argument selects a mic by part of its name, e.g. `scan "A50 Mic"`.

## Config

Saved to `%APPDATA%\no.diysoundmixer.app\config.json` one second after a change, and when the app exits.
