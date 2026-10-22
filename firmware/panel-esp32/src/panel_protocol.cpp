#include "panel_protocol.h"

#include <ArduinoJson.h>

#include <cstdlib>

namespace panel {

namespace {

uint32_t parse_color(const char* hex) {
    if (!hex || hex[0] != '#') return 0x808080;
    return static_cast<uint32_t>(std::strtoul(hex + 1, nullptr, 16)) & 0xFFFFFF;
}

int base64_value(char c) {
    if (c >= 'A' && c <= 'Z') return c - 'A';
    if (c >= 'a' && c <= 'z') return c - 'a' + 26;
    if (c >= '0' && c <= '9') return c - '0' + 52;
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

std::vector<uint8_t> base64_decode(const char* text) {
    std::vector<uint8_t> out;
    uint32_t bits = 0;
    int count = 0;
    for (const char* p = text; p && *p; ++p) {
        int v = base64_value(*p);
        if (v < 0) continue;  // padding or junk
        bits = (bits << 6) | static_cast<uint32_t>(v);
        count += 6;
        if (count >= 8) {
            count -= 8;
            out.push_back(static_cast<uint8_t>((bits >> count) & 0xFF));
        }
    }
    return out;
}

std::vector<std::string> strings(JsonArrayConst array) {
    std::vector<std::string> out;
    for (JsonVariantConst v : array) out.emplace_back(v | "");
    return out;
}

std::string to_line(const JsonDocument& doc) {
    std::string out;
    serializeJson(doc, out);
    return out;
}

}  // namespace

Update handle_line(const std::string& line, Model& model) {
    Update update;
    JsonDocument doc;
    if (deserializeJson(doc, line) != DeserializationError::Ok) return update;
    const std::string type = doc["t"] | "";

    if (type == "hello?") {
        update.replies.push_back(hello_line());
    } else if (type == "ch") {
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
        ch.capture = doc["cap"] | false;
        update.channels |= 1 << i;
    } else if (type == "meter") {
        JsonArrayConst values = doc["v"];
        for (int i = 0; i < kSlots && i < static_cast<int>(values.size()); ++i) {
            model.meters[i] = values[i] | 0;
        }
        update.meters = true;
    } else if (type == "lang") {
        model.norwegian = std::string(doc["v"] | "en") == "no";
        update.language = true;
        update.channels = (1 << kSlots) - 1;  // mute and "no device" texts
    } else if (type == "apps") {
        model.apps.clear();
        for (JsonObjectConst item : doc["list"].as<JsonArrayConst>()) {
            App app;
            app.id = item["id"] | "";
            app.name = item["name"] | "";
            app.slot = item["ch"] | kUncontrolled;
            app.automatic = item["auto"] | false;
            app.has_icon = item["icon"] | false;
            if (!app.id.empty()) model.apps.push_back(std::move(app));
        }
        update.apps = true;
    } else if (type == "icon") {
        Icon icon;
        icon.width = doc["w"] | 0;
        icon.height = doc["h"] | 0;
        icon.pixels = base64_decode(doc["px"] | "");
        const size_t expected = size_t(icon.width) * icon.height * 3;
        if (icon.width > 0 && icon.height > 0 && icon.pixels.size() == expected) {
            model.icons[doc["id"] | ""] = std::move(icon);
            update.icons = true;
        }
    } else if (type == "devices") {
        Devices& d = model.devices;
        d.present = true;
        d.outputs = strings(doc["out"]);
        d.inputs = strings(doc["in"]);
        JsonArrayConst sel = doc["sel"];
        for (int i = 0; i < kSlots; ++i) d.selected[i] = i < static_cast<int>(sel.size()) ? (sel[i] | "") : "";
        d.routing = doc["routing"] | false;
        update.devices = true;
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
    return to_line(doc);
}

std::string touch_mute_line(int slot) {
    JsonDocument doc;
    doc["t"] = "touch";
    doc["action"] = "mute";
    doc["ch"] = slot;
    return to_line(doc);
}

std::string assign_line(const std::string& app, int slot) {
    JsonDocument doc;
    doc["t"] = "assign";
    doc["app"] = app;
    if (slot == kUncontrolled) {
        doc["to"] = "none";
    } else if (slot == kAutomatic) {
        doc["to"] = "auto";
    } else {
        doc["to"] = slot;
    }
    return to_line(doc);
}

std::string device_line(int slot, const std::string& name) {
    JsonDocument doc;
    doc["t"] = "device";
    doc["ch"] = slot;
    doc["name"] = name;
    return to_line(doc);
}

}  // namespace panel
