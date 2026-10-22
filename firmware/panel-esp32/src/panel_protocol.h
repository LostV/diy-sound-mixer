// The screen's side of the device protocol (protocol/README.md).
// Shared by the CrowPanel firmware and the desktop simulator: transports
// only move lines; this turns them into screen state and back.

#pragma once

#include <array>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

namespace panel {

constexpr int kSlots = 4;
constexpr const char* kFirmware = "0.2.0";

/// `assign` targets besides a channel slot.
constexpr int kUncontrolled = -1;
constexpr int kAutomatic = -2;

struct Channel {
    bool present = false;  // false until the PC has sent this slot
    std::string name;
    uint32_t color = 0x808080;  // 0xRRGGBB
    std::string sub;            // apps in the channel, or the mic name
    int volume = 0;             // 0..100
    bool muted = false;
    bool available = true;
    bool capture = false;  // the microphone channel; takes no apps
};

/// An app on the app page.
struct App {
    std::string id;  // process name; sent back in `assign`
    std::string name;
    int slot = kUncontrolled;
    bool automatic = false;  // placed by the rules
    bool has_icon = false;
};

/// An app icon in LVGL's RGB565A8 layout: w*h RGB565, then w*h alpha.
struct Icon {
    uint16_t width = 0;
    uint16_t height = 0;
    std::vector<uint8_t> pixels;
};

struct Devices {
    bool present = false;
    std::vector<std::string> outputs;
    std::vector<std::string> inputs;
    std::array<std::string, kSlots> selected;  // "" = Windows default
    bool routing = false;                      // per-app outputs possible
};

struct Model {
    std::array<Channel, kSlots> channels;
    bool norwegian = false;  // the screen's own texts; English until the PC says otherwise
    std::array<int, kSlots> meters{};  // 0..100, already dB scaled by the PC
    std::vector<App> apps;
    std::map<std::string, Icon> icons;  // by app id; nodes never move
    Devices devices;
};

/// What changed after handling a line, so the UI only redraws that.
struct Update {
    uint8_t channels = 0;  // bit i set: channel i changed
    bool meters = false;
    bool language = false;
    bool apps = false;
    bool icons = false;
    bool devices = false;
    std::vector<std::string> replies;  // lines to send back (e.g. hello)
};

/// Applies one line from the PC to `model`. Bad lines are ignored.
Update handle_line(const std::string& line, Model& model);

std::string hello_line(const char* device = "panel");
std::string touch_mute_line(int slot);
/// `slot` is a channel slot, kUncontrolled or kAutomatic.
std::string assign_line(const std::string& app, int slot);
/// `name` "" picks the Windows default.
std::string device_line(int slot, const std::string& name);

}  // namespace panel
