// The 800x480 mixer screen, design A2 (edge-lit glass, docs/design/).
// Pure LVGL: the same code runs on the CrowPanel and in the desktop simulator.

#pragma once

#include "panel_protocol.h"

namespace ui {

using TouchMuteFn = void (*)(int slot);

/// Builds the screen on the active display. `on_touch_mute` is called when a
/// channel is tapped.
void init(TouchMuteFn on_touch_mute);

/// Shows or hides the "waiting for the PC app" state.
void set_connected(bool connected);

/// Redraws what `update` says changed.
void apply(const panel::Model& model, const panel::Update& update);

}  // namespace ui
