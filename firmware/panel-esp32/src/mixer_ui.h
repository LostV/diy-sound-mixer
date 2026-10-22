// The 800x480 mixer screen, design A2 (edge-lit glass, docs/design/).
// Pure LVGL: the same code runs on the CrowPanel and in the desktop simulator.
//
// Two pages, switched with tabs at the top: the mixer (rings, volume, mute)
// and the apps (drag apps between channels, pick output devices).

#pragma once

#include <string>

#include "panel_protocol.h"

namespace ui {

/// Sends one protocol line to the PC (touch, assign, device).
using SendFn = void (*)(const std::string& line);

/// Builds the screen on the active display.
void init(SendFn send);

/// Shows or hides the "waiting for the PC app" state. Call with false
/// *before* clearing the model: it drops everything that points into it.
void set_connected(bool connected);

/// Redraws what `update` says changed. `model` must outlive the next call.
void apply(const panel::Model& model, const panel::Update& update);

}  // namespace ui
