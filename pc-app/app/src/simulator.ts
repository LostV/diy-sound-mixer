// Simulated hardware: the 5" screen plus four knobs with mute buttons.
// Speaks the device protocol (protocol/README.md) exactly like the Pico and
// the CrowPanel will, so the PC side cannot tell the difference.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const SLOTS = 4;
const FIRMWARE = "sim-0.1.0";
const DRAG_PX_PER_DETENT = 10;
const WHEEL_PX_PER_DETENT = 100;
const DEGREES_PER_DETENT = 15; // 24 detents per turn, like the PEC11R
const LOG_LIMIT = 300;

interface Channel {
  name: string;
  vol: number;
  mute: boolean;
  color: string;
  sub: string;
  avail: boolean;
}

type HostMessage =
  | { t: "hello?" }
  | { t: "led"; id: number; on: boolean }
  | ({ t: "ch"; i: number } & Channel)
  | { t: "meter"; v: number[] };

const channels: (Channel | null)[] = Array(SLOTS).fill(null);
const meters: number[] = Array(SLOTS).fill(0);
const leds: boolean[] = Array(SLOTS).fill(false);

const screenEl = document.querySelector<HTMLElement>("#screen")!;
const controlsEl = document.querySelector<HTMLElement>("#controls")!;
const logEl = document.querySelector<HTMLOListElement>("#log")!;
const logMeter = document.querySelector<HTMLInputElement>("#log-meter")!;
document.querySelector("#log-clear")!.addEventListener("click", () => logEl.replaceChildren());

// ---- protocol ----

function send(message: object) {
  const line = JSON.stringify(message);
  log("out", line);
  invoke("sim_send", { line });
}

function sendHello() {
  send({ t: "hello", dev: "sim", fw: FIRMWARE, proto: 1 });
}

function receive(line: string) {
  let message: HostMessage;
  try {
    message = JSON.parse(line);
  } catch {
    log("in", `ugyldig: ${line}`);
    return;
  }
  if (message.t !== "meter" || logMeter.checked) log("in", line);

  switch (message.t) {
    case "hello?":
      sendHello();
      break;
    case "ch": {
      const { i, ...channel } = message;
      if (i < SLOTS) channels[i] = channel;
      renderColumn(i);
      break;
    }
    case "meter":
      message.v.slice(0, SLOTS).forEach((level, i) => (meters[i] = level));
      renderMeters();
      break;
    case "led":
      if (message.id < SLOTS) leds[message.id] = message.on;
      renderLeds();
      break;
  }
}

function log(direction: "in" | "out", text: string) {
  const item = document.createElement("li");
  item.className = direction;
  item.textContent = `${direction === "in" ? "PC → enhet" : "enhet → PC"}  ${text}`;
  logEl.prepend(item);
  while (logEl.childElementCount > LOG_LIMIT) logEl.lastElementChild!.remove();
}

// ---- screen ----

const RING_RADIUS = 42;
const METER_RADIUS = 34;
const SWEEP = 0.75; // 270° gauge

function arc(radius: number, className: string) {
  const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  circle.setAttribute("cx", "50");
  circle.setAttribute("cy", "50");
  circle.setAttribute("r", String(radius));
  circle.setAttribute("class", className);
  const length = 2 * Math.PI * radius;
  circle.style.strokeDasharray = `${length * SWEEP} ${length}`;
  return circle;
}

function setArc(circle: SVGCircleElement, radius: number, fraction: number) {
  const length = 2 * Math.PI * radius;
  circle.style.strokeDasharray = `${length * SWEEP * Math.max(0, Math.min(1, fraction))} ${length}`;
}

interface ColumnView {
  root: HTMLButtonElement;
  ring: SVGCircleElement;
  meter: SVGCircleElement;
  value: HTMLElement;
  name: HTMLElement;
  sub: HTMLElement;
}

const columns: ColumnView[] = [];

for (let i = 0; i < SLOTS; i++) {
  const root = document.createElement("button");
  root.type = "button";
  root.className = "column empty";
  root.addEventListener("click", () => send({ t: "touch", action: "mute", ch: i }));

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("class", "gauge");
  const track = arc(RING_RADIUS, "track");
  const ring = arc(RING_RADIUS, "ring");
  const meterTrack = arc(METER_RADIUS, "meter-track");
  const meter = arc(METER_RADIUS, "meter");
  svg.append(track, ring, meterTrack, meter);

  const value = document.createElement("div");
  value.className = "gauge-value";
  const gaugeWrap = document.createElement("div");
  gaugeWrap.className = "gauge-wrap";
  gaugeWrap.append(svg, value);

  const name = document.createElement("div");
  name.className = "name";
  const sub = document.createElement("div");
  sub.className = "sub";

  root.append(gaugeWrap, name, sub);
  screenEl.append(root);
  columns.push({ root, ring, meter, value, name, sub });
}

function renderColumn(i: number) {
  const view = columns[i];
  const ch = channels[i];
  if (!view) return;
  view.root.classList.toggle("empty", !ch);
  if (!ch) return;
  view.root.style.setProperty("--accent", ch.color);
  view.root.classList.toggle("muted", ch.mute);
  view.root.classList.toggle("unavailable", !ch.avail);
  view.root.setAttribute("aria-label", `${ch.name} ${ch.vol} prosent${ch.mute ? ", mutet" : ""}. Trykk for mute.`);
  setArc(view.ring, RING_RADIUS, ch.vol / 100);
  view.value.textContent = ch.avail ? (ch.mute ? "MUTE" : String(ch.vol)) : "–";
  view.name.textContent = ch.name;
  view.sub.textContent = ch.avail ? ch.sub : "Ingen enhet";
}

function renderMeters() {
  columns.forEach((view, i) => setArc(view.meter, METER_RADIUS, meters[i] / 100));
}

// ---- knobs and buttons ----

interface KnobView {
  knob: HTMLElement;
  led: HTMLElement;
  angle: number;
}

const knobs: KnobView[] = [];

function turn(i: number, detents: number) {
  if (detents === 0) return;
  const view = knobs[i];
  view.angle += detents * DEGREES_PER_DETENT;
  view.knob.style.setProperty("--angle", `${view.angle}deg`);
  send({ t: "enc", ch: i, d: detents });
}

function press(id: string) {
  send({ t: "btn", id, s: 1 });
}

function release(id: string) {
  send({ t: "btn", id, s: 0 });
}

for (let i = 0; i < SLOTS; i++) {
  const slot = document.createElement("div");
  slot.className = "slot";

  const knob = document.createElement("div");
  knob.className = "knob";
  knob.tabIndex = 0;
  knob.setAttribute("role", "slider");
  knob.setAttribute("aria-label", `Knott ${i + 1}`);
  const cap = document.createElement("div");
  cap.className = "knob-cap";
  knob.append(cap);

  // Wheel: one notch (~100 px) = one detent; scrolling up turns clockwise.
  // Accumulated so fast spins and touchpads give the right number of detents.
  let wheel = 0;
  knob.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const px = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * 33 : e.deltaY;
      wheel -= px;
      const detents = Math.trunc(wheel / WHEEL_PX_PER_DETENT);
      wheel -= detents * WHEEL_PX_PER_DETENT;
      turn(i, detents);
    },
    { passive: false },
  );

  // Drag up/down to turn; a click without movement is a push.
  let startY = 0;
  let sentDetents = 0;
  let moved = false;
  knob.addEventListener("pointerdown", (e) => {
    knob.setPointerCapture(e.pointerId);
    startY = e.clientY;
    sentDetents = 0;
    moved = false;
  });
  knob.addEventListener("pointermove", (e) => {
    if (!knob.hasPointerCapture(e.pointerId)) return;
    const detents = Math.trunc((startY - e.clientY) / DRAG_PX_PER_DETENT);
    if (detents !== sentDetents) {
      moved = true;
      turn(i, detents - sentDetents);
      sentDetents = detents;
    }
  });
  knob.addEventListener("pointerup", (e) => {
    knob.releasePointerCapture(e.pointerId);
    if (!moved) {
      press(`enc${i}`);
      release(`enc${i}`);
    }
  });
  knob.addEventListener("keydown", (e) => {
    if (e.key === "ArrowUp" || e.key === "ArrowRight") turn(i, 1);
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") turn(i, -1);
    else if (e.key === "Enter" || e.key === " ") press(`enc${i}`);
    else return;
    e.preventDefault();
  });
  knob.addEventListener("keyup", (e) => {
    if (e.key === "Enter" || e.key === " ") release(`enc${i}`);
  });

  const button = document.createElement("button");
  button.type = "button";
  button.className = "mute-key";
  button.setAttribute("aria-label", `Mute-knapp ${i + 1}`);
  const led = document.createElement("span");
  led.className = "led";
  button.append(led);
  button.addEventListener("pointerdown", () => press(`mute${i}`));
  button.addEventListener("pointerup", () => release(`mute${i}`));
  button.addEventListener("keydown", (e) => {
    if ((e.key === "Enter" || e.key === " ") && !e.repeat) {
      e.preventDefault();
      press(`mute${i}`);
    }
  });
  button.addEventListener("keyup", (e) => {
    if (e.key === "Enter" || e.key === " ") release(`mute${i}`);
  });

  slot.append(knob, button);
  controlsEl.append(slot);
  knobs.push({ knob, led, angle: 0 });
}

function renderLeds() {
  knobs.forEach((view, i) => view.led.classList.toggle("on", leds[i]));
}

// ---- start ----

listen<string[]>("sim-host", ({ payload }) => payload.forEach(receive)).then(sendHello);
