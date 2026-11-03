// CrowPanel 5" ESP32-S3 IPS (Elecrow DLE06050B) build of the panel firmware:
// the same LVGL screen as the simulator, on the RGB panel with GT911 touch,
// connected to the PC app over the board's USB-C port (a CH340C on UART0).
//
// Pins and the auto-reset circuit come from Elecrow's schematic
// (5inch_ESP32-S3_Scehmeatic.pdf). Build and upload: see README.md.

#include <Arduino.h>
#include <Wire.h>
#include <esp_heap_caps.h>
#include <esp_lcd_panel_ops.h>
#include <esp_lcd_panel_rgb.h>

#include <string>

#include "lvgl.h"
#include "src/mixer_ui.h"
#include "src/panel_protocol.h"

namespace {

constexpr int kWidth = 800;
constexpr int kHeight = 480;

// RGB panel (ST7282). Data pins in esp_lcd order: B0-B4, G0-G5, R0-R4.
constexpr int kDataPins[16] = {8, 3, 46, 9, 1, 5, 6, 7, 15, 16, 4, 45, 48, 47, 21, 14};
constexpr int kPclk = 42, kHsync = 39, kVsync = 41, kDe = 40;
constexpr uint32_t kPixelClock = 15 * 1000 * 1000;  // Elecrow's examples; lower it if the picture shimmers
constexpr int kBacklight = 2;                       // EN/PWM of the backlight driver

// GT911 touch on I2C. INT is held low during reset to pick address 0x5D.
constexpr int kTouchSda = 19, kTouchScl = 20, kTouchInt = 18, kTouchRst = 38;

// Protocol over UART0. The PC app opens the CH340 at the same rate.
constexpr uint32_t kBaud = 921600;
constexpr size_t kMaxLine = 32 * 1024;  // longer lines are dropped
// The PC sends meters about 30 times a second, so silence means it is gone.
constexpr uint32_t kSilenceMs = 3000;

constexpr int kDrawLines = 40;

esp_lcd_panel_handle_t panel = nullptr;
panel::Model model;

// ---- Display ----

void start_panel() {
    esp_lcd_rgb_panel_config_t cfg = {};
    cfg.clk_src = LCD_CLK_SRC_DEFAULT;
    cfg.timings.pclk_hz = kPixelClock;
    cfg.timings.h_res = kWidth;
    cfg.timings.v_res = kHeight;
    cfg.timings.hsync_pulse_width = 4;
    cfg.timings.hsync_back_porch = 43;
    cfg.timings.hsync_front_porch = 8;
    cfg.timings.vsync_pulse_width = 4;
    cfg.timings.vsync_back_porch = 12;
    cfg.timings.vsync_front_porch = 8;
    cfg.timings.flags.pclk_active_neg = 1;
    cfg.data_width = 16;
    cfg.bits_per_pixel = 16;
    cfg.num_fbs = 1;
    // The frame buffer is in PSRAM; bounce buffers in internal RAM keep the
    // picture steady while the CPU also uses PSRAM.
    cfg.bounce_buffer_size_px = kWidth * 10;
    cfg.hsync_gpio_num = kHsync;
    cfg.vsync_gpio_num = kVsync;
    cfg.de_gpio_num = kDe;
    cfg.pclk_gpio_num = kPclk;
    cfg.disp_gpio_num = -1;  // DISP is pulled up on the board
    for (int i = 0; i < 16; i++) cfg.data_gpio_nums[i] = kDataPins[i];
    cfg.flags.fb_in_psram = 1;
    ESP_ERROR_CHECK(esp_lcd_new_rgb_panel(&cfg, &panel));
    ESP_ERROR_CHECK(esp_lcd_panel_reset(panel));
    ESP_ERROR_CHECK(esp_lcd_panel_init(panel));
}

// Copies the rendered area into the frame buffer; the panel scans it out.
void flush(lv_display_t* display, const lv_area_t* area, uint8_t* pixels) {
    esp_lcd_panel_draw_bitmap(panel, area->x1, area->y1, area->x2 + 1, area->y2 + 1, pixels);
    lv_display_flush_ready(display);
}

// ---- Touch ----

uint8_t touch_addr = 0x5D;
int touch_max_x = kWidth, touch_max_y = kHeight;

bool touch_read(uint16_t reg, uint8_t* buf, size_t len) {
    Wire.beginTransmission(touch_addr);
    Wire.write(reg >> 8);
    Wire.write(reg & 0xFF);
    if (Wire.endTransmission(false) != 0) return false;
    if (Wire.requestFrom(touch_addr, len) != len) return false;
    for (size_t i = 0; i < len; i++) buf[i] = Wire.read();
    return true;
}

void touch_write(uint16_t reg, uint8_t value) {
    Wire.beginTransmission(touch_addr);
    Wire.write(reg >> 8);
    Wire.write(reg & 0xFF);
    Wire.write(value);
    Wire.endTransmission();
}

void start_touch() {
    pinMode(kTouchRst, OUTPUT);
    pinMode(kTouchInt, OUTPUT);
    digitalWrite(kTouchRst, LOW);
    digitalWrite(kTouchInt, LOW);
    delay(10);
    digitalWrite(kTouchRst, HIGH);
    delay(10);
    pinMode(kTouchInt, INPUT);
    delay(50);
    Wire.begin(kTouchSda, kTouchScl, 400000);

    uint8_t config[4];
    for (uint8_t addr : {0x5D, 0x14}) {
        touch_addr = addr;
        if (touch_read(0x8048, config, sizeof config)) {
            // The touch controller's own resolution, in case it isn't 800x480.
            int x = config[0] | config[1] << 8, y = config[2] | config[3] << 8;
            if (x > 0 && y > 0) touch_max_x = x, touch_max_y = y;
            return;
        }
    }
    touch_addr = 0;  // no touch controller found; the screen still works
}

void read_touch(lv_indev_t*, lv_indev_data_t* data) {
    static lv_point_t point{};
    static bool pressed = false;
    uint8_t status;
    if (touch_addr && touch_read(0x814E, &status, 1) && (status & 0x80)) {
        uint8_t p[4];
        if ((status & 0x0F) > 0 && touch_read(0x8150, p, sizeof p)) {
            point.x = (p[0] | p[1] << 8) * kWidth / touch_max_x;
            point.y = (p[2] | p[3] << 8) * kHeight / touch_max_y;
            pressed = true;
        } else {
            pressed = false;
        }
        touch_write(0x814E, 0);  // ready for the next report
    }
    data->point = point;
    data->state = pressed ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
}

// ---- Link to the PC ----

std::string line;
bool overlong = false;
uint32_t last_heard = 0;
bool connected = false;

void send_to_pc(const std::string& out) {
    Serial.write(out.data(), out.size());
    Serial.write('\n');
}

void handle(const std::string& text) {
    panel::Update update = panel::handle_line(text, model);
    for (const std::string& reply : update.replies) send_to_pc(reply);
    ui::apply(model, update);
}

void poll_serial() {
    bool heard = false;
    while (Serial.available() > 0) {
        char c = static_cast<char>(Serial.read());
        heard = true;
        if (c == '\n') {
            if (!overlong && !line.empty()) {
                if (!connected) {
                    connected = true;
                    ui::set_connected(true);
                    // After a silence the model was cleared: ask for everything.
                    send_to_pc(panel::hello_line());
                }
                handle(line);
            }
            line.clear();
            overlong = false;
        } else if (c != '\r') {
            if (line.size() < kMaxLine) line += c;
            else overlong = true;
        }
    }
    uint32_t now = millis();
    if (heard) last_heard = now;
    if (connected && now - last_heard > kSilenceMs) {
        connected = false;
        ui::set_connected(false);  // first: it drops what points into the model
        model = panel::Model{};
    }
}

uint32_t ticks() { return millis(); }

}  // namespace

void setup() {
    Serial.setRxBufferSize(16 * 1024);
    Serial.begin(kBaud);

    start_panel();
    start_touch();

    lv_init();
    lv_tick_set_cb(ticks);
    lv_display_t* display = lv_display_create(kWidth, kHeight);
    size_t bytes = kWidth * kDrawLines * 2;
    void* buf1 = heap_caps_malloc(bytes, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    void* buf2 = heap_caps_malloc(bytes, MALLOC_CAP_INTERNAL | MALLOC_CAP_8BIT);
    lv_display_set_buffers(display, buf1, buf2, bytes, LV_DISPLAY_RENDER_MODE_PARTIAL);
    lv_display_set_flush_cb(display, flush);

    lv_indev_t* touch = lv_indev_create();
    lv_indev_set_type(touch, LV_INDEV_TYPE_POINTER);
    lv_indev_set_read_cb(touch, read_touch);

    ui::init(send_to_pc);
    lv_timer_handler();  // draw the first frame before the light comes on

    ledcAttach(kBacklight, 5000, 8);
    ledcWrite(kBacklight, 255);

    // Introduce ourselves; the PC also asks with hello? when it opens the port.
    send_to_pc(panel::hello_line());
}

void loop() {
    poll_serial();
    uint32_t wait = lv_timer_handler();
    delay(wait < 5 ? wait : 5);
}
