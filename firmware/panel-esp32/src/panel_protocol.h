// The screen's side of the device protocol (protocol/README.md).
// Shared by the CrowPanel firmware and the desktop simulator: transports
// only move lines; this turns them into screen state and back.

#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace panel {

constexpr int kSlots = 4;
constexpr const char* kFirmware = "0.1.0";

struct Channel {
    bool present = false;  // false until the PC has sent this slot
    std::string name;
    uint32_t color = 0x808080;  // 0xRRGGBB
    std::string sub;            // apps in the channel, or the mic name
    int volume = 0;             // 0..100
    bool muted = false;
    bool available = true;
};

struct Model {
    std::array<Channel, kSlots> channels;
    std::array<int, kSlots> meters{};  // 0..100, already dB scaled by the PC
};

/// What changed after handling a line, so the UI only redraws that.
struct Update {
    uint8_t channels = 0;  // bit i set: channel i changed
    bool meters = false;
    std::vector<std::string> replies;  // lines to send back (e.g. hello)
};

/// Applies one line from the PC to `model`. Bad lines are ignored.
Update handle_line(const std::string& line, Model& model);

std::string hello_line(const char* device = "panel");
std::string touch_mute_line(int slot);

}  // namespace panel
