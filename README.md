# DIY Sound Mixer

A homemade hardware mixer for Windows (BEACN/Sonar style): physical knobs and a 5" screen that control per-app audio across four channels, **Spill, Media, Chat and Mic**, with software we own end to end.

## Structure

| Folder | Contents |
|---|---|
| `docs/` | Plans and shopping lists ([software plan](docs/software-plan.md), [hardware options](docs/hardware-alternativer.md), [prototype shopping list](docs/handleliste-prototype.md)) |
| `pc-app/` | PC app in Rust + Tauri 2 (audio engine, tray, settings) |
| `firmware/` | Firmware for the Pico 2 (knobs) and CrowPanel 5" (screen) |
| `protocol/` | Protocol between the PC and the devices |

## Status

Phase 1: audio engine (channels with per-app volume) – in progress.
