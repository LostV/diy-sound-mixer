// Simulated hardware: the 5" screen, four push knobs and the mic mute button.
// Speaks the device protocol (protocol/README.md) exactly like the Pico and
// the CrowPanel do, so the PC side cannot tell the difference.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { type Language, language, setLanguage, tr } from "./i18n";

const SLOTS = 4;
const MIC_SLOT = 3; // the mic mute button is wired as mute3 with LED 3
const FIRMWARE = "sim-0.2.0";
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
  cap?: boolean;
}

type HostMessage =
  | { t: "hello?" }
  | { t: "led"; id: number; on: boolean }
  | ({ t: "ch"; i: number } & Channel)
  | { t: "meter"; v: number[] }
  | { t: "lang"; v: Language };

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
    log("in", `${tr().invalid}: ${line}`);
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
      renderCard(i);
      break;
    }
    case "meter":
      message.v.slice(0, SLOTS).forEach((level, i) => (meters[i] = level));
      renderMeters();
      break;
    case "led":
      if (message.id < SLOTS) leds[message.id] = message.on;
      renderLed();
      break;
    case "lang":
      // Like the panel: only the device's own texts change; names come in `ch`.
      setLanguage(message.v);
      translateControls();
      for (let i = 0; i < SLOTS; i++) renderCard(i);
      break;
  }
}

function log(direction: "in" | "out", text: string) {
  const item = document.createElement("li");
  item.className = direction;
  item.textContent = `${direction === "in" ? tr().logIn : tr().logOut}  ${text}`;
  logEl.prepend(item);
  while (logEl.childElementCount > LOG_LIMIT) logEl.lastElementChild!.remove();
}

// ---- screen: the panel firmware's mixer page (firmware/panel-esp32) ----

const GAUGE = 176; // px on the 800×480 panel
const RING_RADIUS = 82;
const METER_RADIUS = 66;
const SWEEP = 0.75; // 270° gauge

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, parent?: HTMLElement) {
  const node = document.createElement(tag);
  node.className = className;
  parent?.append(node);
  return node;
}

function arc(radius: number, className: string) {
  const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  circle.setAttribute("cx", String(GAUGE / 2));
  circle.setAttribute("cy", String(GAUGE / 2));
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

interface CardView {
  root: HTMLButtonElement;
  ring: SVGCircleElement;
  meter: SVGCircleElement;
  value: HTMLElement;
  name: HTMLElement;
  info: HTMLElement;
}

const title = el("div", "screen-title", screenEl);
title.textContent = "DIY Sound Mixer";
const cardsEl = el("div", "cards", screenEl);
const cards: CardView[] = [];

for (let i = 0; i < SLOTS; i++) {
  const root = el("button", "card empty", cardsEl);
  root.type = "button";
  root.addEventListener("click", () => send({ t: "touch", action: "mute", ch: i }));

  el("span", "dot", root);
  const name = el("span", "name", root);

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${GAUGE} ${GAUGE}`);
  svg.setAttribute("class", "gauge");
  svg.setAttribute("aria-hidden", "true");
  const ring = arc(RING_RADIUS, "ring");
  const meter = arc(METER_RADIUS, "meter");
  svg.append(arc(RING_RADIUS, "track"), ring, arc(METER_RADIUS, "meter-track"), meter);
  root.append(svg);

  const value = el("span", "gauge-value", root);
  const info = el("span", "info", root);
  cards.push({ root, ring, meter, value, name, info });
}

/// The screen lists the channel's apps under the dial. The simulator only
/// gets the short subtitle ("Spotify, Chrome +2"), so it unpacks that.
function renderInfo(view: CardView, ch: Channel) {
  view.info.replaceChildren();
  if (ch.cap) {
    const device = el("span", "mic-name", view.info);
    device.textContent = ch.avail ? ch.sub : tr().noDevice;
    return;
  }
  const match = /^(.*?)(?: \+(\d+))?$/.exec(ch.sub)!;
  const names = match[1] ? match[1].split(", ") : [];
  for (const appName of names) {
    const row = el("span", "app", view.info);
    el("span", "avatar", row).textContent = appName.charAt(0);
    el("span", "app-name", row).textContent = appName;
  }
  if (match[2]) el("span", "more", view.info).textContent = tr().moreApps(Number(match[2]));
}

function renderCard(i: number) {
  const view = cards[i];
  const ch = channels[i];
  if (!view) return;
  view.root.classList.toggle("empty", !ch);
  if (!ch) return;
  view.root.style.setProperty("--accent", ch.color);
  view.root.classList.toggle("muted", ch.mute);
  view.root.classList.toggle("unavailable", !ch.avail);
  view.root.setAttribute("aria-label", tr().columnAria(ch.name, ch.vol, ch.mute));
  setArc(view.ring, RING_RADIUS, ch.avail ? ch.vol / 100 : 0);
  view.value.textContent = ch.mute ? tr().mutedLabel : ch.avail ? String(ch.vol) : "–";
  view.name.textContent = ch.name;
  renderInfo(view, ch);
}

function renderMeters() {
  cards.forEach((view, i) => setArc(view.meter, METER_RADIUS, meters[i] / 100));
}

// ---- knobs and the mic button ----

interface KnobView {
  knob: HTMLElement;
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
  const slot = el("div", "slot", controlsEl);
  const knob = el("div", "knob", slot);
  knob.tabIndex = 0;
  knob.setAttribute("role", "slider");
  el("div", "knob-cap", knob);

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

  // Drag up/down to turn; a click without movement is a push (= mute).
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

  knobs.push({ knob, angle: 0 });
}

// The one dedicated button: mic mute, with its red LED.
const micButton = el("button", "mic-key", controlsEl);
micButton.type = "button";
const micLed = el("span", "led", micButton);
const micId = `mute${MIC_SLOT}`;
micButton.addEventListener("pointerdown", () => press(micId));
micButton.addEventListener("pointerup", () => release(micId));
micButton.addEventListener("keydown", (e) => {
  if ((e.key === "Enter" || e.key === " ") && !e.repeat) {
    e.preventDefault();
    press(micId);
  }
});
micButton.addEventListener("keyup", (e) => {
  if (e.key === "Enter" || e.key === " ") release(micId);
});

function translateControls() {
  knobs.forEach((view, i) => view.knob.setAttribute("aria-label", tr().knob(i + 1)));
  micButton.setAttribute("aria-label", tr().micKey);
}

function renderLed() {
  micLed.classList.toggle("on", leds[MIC_SLOT]);
  micButton.setAttribute("aria-pressed", String(leds[MIC_SLOT]));
}

// ---- start ----

setLanguage(language());
translateControls();

listen<string[]>("sim-host", ({ payload }) => payload.forEach(receive)).then(sendHello);
