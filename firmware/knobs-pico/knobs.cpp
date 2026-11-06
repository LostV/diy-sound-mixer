// Knob firmware for the Raspberry Pi Pico 2 W (Arduino-Pico core).
//
// Reads 4 rotary encoders with push, 4 mute buttons, and drives 4 mute LEDs.
// Speaks protocol v1 (protocol/README.md) over USB serial: it only reports
// input and switches LEDs, the PC app decides everything else.

#include <Arduino.h>
#include <ArduinoJson.h>

static const char* FW_VERSION = "0.2.0";
static const int CHANNELS = 4;

// Pins, laid out for the shell: in it the Pico lies upside down with USB to
// the right, so each knob gets the pins nearest to it (knob 0, far left, at
// pin 20's end) and the 15 cm wires reach. Only channel 3 (Mic) has a mute
// button and LED; the others' are free pins. GP23, GP24, GP25 and GP29
// belong to the wireless chip on the W board.
static const uint8_t ENC_A[CHANNELS] = {12, 8, 4, 0};
static const uint8_t ENC_B[CHANNELS] = {13, 9, 5, 1};
static const uint8_t ENC_SW[CHANNELS] = {14, 10, 6, 2};
static const uint8_t MUTE_BTN[CHANNELS] = {3, 7, 11, 28};
static const uint8_t MUTE_LED[CHANNELS] = {16, 17, 18, 27};

// Encoder 41017819 has 24 detents and goes through a full quadrature cycle
// (4 edges) per detent, resting with both pins high. Set HALF_STEP if one
// click is reported as two (an encoder with half as many pulses as detents).
static const bool HALF_STEP = false;
// Swaps the direction of all knobs. The 41017819 counted backwards on the
// first breadboard, so all knobs must be wired the same way as knob 0.
static const bool REVERSE = true;

static const uint32_t DEBOUNCE_MS = 10;

// ---------------------------------------------------------------- encoders

// Valid quadrature transitions, indexed by (previous state << 2) | state.
// Bounce produces invalid jumps (0) or steps that cancel each other out.
static const int8_t QUAD_TABLE[16] = {0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0};

struct Encoder {
    volatile uint8_t state = 3;
    volatile int8_t steps = 0;    // edges since the last rest position
    volatile int32_t detents = 0; // not yet reported to the PC
};

static Encoder encoders[CHANNELS];

static void update_encoder(int i) {
    Encoder& e = encoders[i];
    uint8_t now = (digitalRead(ENC_A[i]) << 1) | digitalRead(ENC_B[i]);
    if (now == e.state) return;
    e.steps += QUAD_TABLE[(e.state << 2) | now];
    e.state = now;

    bool at_rest = now == 3 || (HALF_STEP && now == 0);
    if (!at_rest) return;
    // A real detent covers at least half its edges; anything less is bounce.
    int threshold = HALF_STEP ? 1 : 2;
    if (e.steps >= threshold) e.detents++;
    else if (e.steps <= -threshold) e.detents--;
    e.steps = 0;
}

static void enc0_isr() { update_encoder(0); }
static void enc1_isr() { update_encoder(1); }
static void enc2_isr() { update_encoder(2); }
static void enc3_isr() { update_encoder(3); }
static void (*const ENC_ISR[CHANNELS])() = {enc0_isr, enc1_isr, enc2_isr, enc3_isr};

static int32_t take_detents(int i) {
    noInterrupts();
    int32_t d = encoders[i].detents;
    encoders[i].detents = 0;
    interrupts();
    return REVERSE ? -d : d;
}

// ----------------------------------------------------------------- buttons

struct Button {
    uint8_t pin;
    char id[6];
    bool pressed = false;   // debounced state
    bool raw = false;
    uint32_t changed_at = 0;
};

static Button buttons[CHANNELS * 2];

// Returns true when the debounced state changed.
static bool poll_button(Button& b, uint32_t now) {
    bool raw = digitalRead(b.pin) == LOW; // pressed pulls the pin to ground
    if (raw != b.raw) {
        b.raw = raw;
        b.changed_at = now;
    }
    if (b.raw != b.pressed && now - b.changed_at >= DEBOUNCE_MS) {
        b.pressed = b.raw;
        return true;
    }
    return false;
}

// ---------------------------------------------------------------- protocol

static void send(const JsonDocument& doc) {
    serializeJson(doc, Serial);
    Serial.write('\n');
}

static void send_hello() {
    JsonDocument doc;
    doc["t"] = "hello";
    doc["dev"] = "knobs";
    doc["fw"] = FW_VERSION;
    doc["proto"] = 1;
    send(doc);
}

static void handle_line(const char* line) {
    JsonDocument doc;
    if (deserializeJson(doc, line)) return; // garbage is ignored
    const char* t = doc["t"] | "";
    if (strcmp(t, "hello?") == 0) {
        send_hello();
    } else if (strcmp(t, "led") == 0) {
        int id = doc["id"] | -1;
        if (id >= 0 && id < CHANNELS) digitalWrite(MUTE_LED[id], (doc["on"] | false) ? HIGH : LOW);
    }
}

static char line_buf[256];
static size_t line_len = 0;

static void read_serial() {
    while (Serial.available()) {
        char c = static_cast<char>(Serial.read());
        if (c == '\n' || c == '\r') {
            if (line_len > 0) {
                line_buf[line_len] = '\0';
                handle_line(line_buf);
            }
            line_len = 0;
        } else if (line_len < sizeof(line_buf) - 1) {
            line_buf[line_len++] = c;
        } else {
            line_len = 0; // too long to be ours, drop it
        }
    }
}

// --------------------------------------------------------------- main loop

static uint32_t blink_until = 0;

// The on-board LED flashes on every input, so wiring can be checked without
// a serial console.
static void blink() {
    digitalWrite(LED_BUILTIN, HIGH);
    blink_until = millis() + 30;
}

void setup() {
    Serial.begin(115200);
    pinMode(LED_BUILTIN, OUTPUT);

    for (int i = 0; i < CHANNELS; i++) {
        pinMode(ENC_A[i], INPUT_PULLUP);
        pinMode(ENC_B[i], INPUT_PULLUP);
        pinMode(MUTE_LED[i], OUTPUT);
        digitalWrite(MUTE_LED[i], LOW);
        encoders[i].state = (digitalRead(ENC_A[i]) << 1) | digitalRead(ENC_B[i]);
        attachInterrupt(digitalPinToInterrupt(ENC_A[i]), ENC_ISR[i], CHANGE);
        attachInterrupt(digitalPinToInterrupt(ENC_B[i]), ENC_ISR[i], CHANGE);

        buttons[i].pin = ENC_SW[i];
        snprintf(buttons[i].id, sizeof(buttons[i].id), "enc%d", i);
        buttons[CHANNELS + i].pin = MUTE_BTN[i];
        snprintf(buttons[CHANNELS + i].id, sizeof(buttons[CHANNELS + i].id), "mute%d", i);
    }
    for (Button& b : buttons) {
        pinMode(b.pin, INPUT_PULLUP);
        b.raw = b.pressed = digitalRead(b.pin) == LOW;
    }
}

void loop() {
    // The device sends hello whenever a PC opens the port.
    static bool connected = false;
    bool port_open = Serial;
    if (port_open != connected) {
        connected = port_open;
        if (connected) send_hello();
    }

    read_serial();

    uint32_t now = millis();
    for (int i = 0; i < CHANNELS; i++) {
        int32_t d = take_detents(i);
        if (d == 0) continue;
        JsonDocument doc;
        doc["t"] = "enc";
        doc["ch"] = i;
        doc["d"] = d;
        send(doc);
        blink();
    }
    for (Button& b : buttons) {
        if (!poll_button(b, now)) continue;
        JsonDocument doc;
        doc["t"] = "btn";
        doc["id"] = b.id;
        doc["s"] = b.pressed ? 1 : 0;
        send(doc);
        blink();
    }

    if (blink_until && static_cast<int32_t>(now - blink_until) >= 0) {
        digitalWrite(LED_BUILTIN, LOW);
        blink_until = 0;
    }
}
