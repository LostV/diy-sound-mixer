#include "panel_protocol.h"

#include <ArduinoJson.h>

#include <cstdlib>

namespace panel {

namespace {

uint32_t parse_color(const char* hex) {
    if (!hex || hex[0] != '#') return 0x808080;
    return static_cast<uint32_t>(std::strtoul(hex + 1, nullptr, 16)) & 0xFFFFFF;
}

}  // namespace

Update handle_line(const std::string& line, Model& model) {
    Update update;
    JsonDocument doc;
    if (deserializeJson(doc, line) != DeserializationError::Ok) return update;
    const char* type = doc["t"] | "";

    if (std::string(type) == "hello?") {
        update.replies.push_back(hello_line());
    } else if (std::string(type) == "ch") {
        int i = doc["i"] | -1;
        if (i < 0 || i >= kSlots) return update;
        Channel& ch = model.channels[i];
        ch.present = true;
        ch.name = doc["name"] | "";
        ch.color = parse_color(doc["color"] | "");
        ch.sub = doc["sub"] | "";
        ch.volume = doc["vol"] | 0;
        ch.muted = doc["mute"] | false;
        ch.available = doc["avail"] | true;
        update.channels |= 1 << i;
    } else if (std::string(type) == "meter") {
        JsonArrayConst values = doc["v"];
        for (int i = 0; i < kSlots && i < static_cast<int>(values.size()); ++i) {
            model.meters[i] = values[i] | 0;
        }
        update.meters = true;
    } else if (std::string(type) == "lang") {
        model.norwegian = std::string(doc["v"] | "en") == "no";
        update.language = true;
        update.channels = (1 << kSlots) - 1;  // mute and "no device" texts
    }
    // "led" is for the knob board; a screen has nothing to do with it.
    return update;
}

std::string hello_line(const char* device) {
    JsonDocument doc;
    doc["t"] = "hello";
    doc["dev"] = device;
    doc["fw"] = kFirmware;
    doc["proto"] = 1;
    std::string out;
    serializeJson(doc, out);
    return out;
}

std::string touch_mute_line(int slot) {
    JsonDocument doc;
    doc["t"] = "touch";
    doc["action"] = "mute";
    doc["ch"] = slot;
    std::string out;
    serializeJson(doc, out);
    return out;
}

}  // namespace panel
