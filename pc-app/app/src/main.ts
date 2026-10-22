import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { type Language, language, setLanguage, tr } from "./i18n";
import type { ChannelSnapshot, MixerSnapshot, SessionSnapshot } from "./types";

/** Ignore incoming volumes this long after the user touched a slider, so it doesn't jump back. */
const USER_INPUT_GRACE_MS = 500;
const DRAG_TYPE = "application/x-diy-mixer-process";

interface ChannelView {
  root: HTMLElement;
  device: HTMLSelectElement;
  deviceKey: string;
  value: HTMLElement;
  fader: HTMLElement;
  faderFill: HTMLElement;
  meter: HTMLElement;
  mute: HTMLButtonElement;
  /** Drop zone with the app chips; null for the mic. */
  chips: HTMLElement | null;
  /** The mic card's info chip (device name); null for playback. */
  micInfo: HTMLElement | null;
  lastUserInput: number;
}

/** One program, possibly with several sessions (one per output device). */
interface AppGroup {
  processName: string;
  displayName: string;
  exePath: string | null;
  assignment: string;
  channel: string | null;
  devices: string[];
}

const channelViews = new Map<string, ChannelView>();
let channelOrder = "";
let sessionsVersion = -1;
let latest: MixerSnapshot | null = null;
/** While dragging, chip lists are not rebuilt (that would cancel the drag). */
let dragging = false;

const channelsEl = document.querySelector<HTMLElement>("#channels")!;
const uncontrolledEl = document.querySelector<HTMLElement>("#uncontrolled")!;
const rowsEl = document.querySelector<HTMLElement>("#session-rows")!;
const menuEl = document.querySelector<HTMLElement>("#chip-menu")!;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Peak 0..1 → 0..1 on a -60..0 dB scale, which looks like a real meter. */
function meterLevel(peak: number) {
  if (peak <= 0.001) return 0;
  return Math.max(0, Math.min(1, (20 * Math.log10(peak) + 60) / 60));
}

// ---- header ----

const autostartBox = document.querySelector<HTMLInputElement>("#autostart")!;
invoke<boolean>("get_autostart").then((on) => (autostartBox.checked = on));
autostartBox.addEventListener("change", async () => {
  try {
    await invoke("set_autostart", { enabled: autostartBox.checked });
  } catch (e) {
    autostartBox.checked = !autostartBox.checked;
    console.error("autostart", e);
  }
});

// ---- language ----

const languageSelect = document.querySelector<HTMLSelectElement>("#language")!;
languageSelect.addEventListener("change", () => invoke("set_language", { language: languageSelect.value }));

/** Retranslates everything; channel cards and chips are rebuilt on the next render. */
function applyLanguage(lang: Language) {
  languageSelect.value = lang;
  if (!setLanguage(lang)) return;
  channelOrder = "";
  sessionsVersion = -1;
  closeMenu();
}
applyLanguage(language());

document.querySelector("#open-simulator")!.addEventListener("click", () => invoke("show_simulator"));

// Custom title bar: the window has no system frame, so the glass reaches the edges.
document.querySelector("#win-min")!.addEventListener("click", () => getCurrentWindow().minimize());
document.querySelector("#win-close")!.addEventListener("click", () => invoke("hide_main"));

// ---- window backdrop ----

type Backdrop = "acrylic" | "mica" | "none";
const BACKDROP_KEY = "backdrop";
const backdropSelect = document.querySelector<HTMLSelectElement>("#backdrop")!;

function readBackdrop(): Backdrop {
  try {
    const saved = localStorage.getItem(BACKDROP_KEY);
    if (saved === "acrylic" || saved === "mica" || saved === "none") return saved;
  } catch {
    // storage unavailable: use the default
  }
  return "acrylic";
}

/** `none` paints the aurora ourselves; the others let Windows draw behind the glass. */
async function applyBackdrop(kind: Backdrop) {
  document.documentElement.dataset.backdrop = kind;
  backdropSelect.value = kind;
  try {
    localStorage.setItem(BACKDROP_KEY, kind);
  } catch {
    // not remembered, still applied
  }
  try {
    await invoke("set_backdrop", { kind });
  } catch (e) {
    console.error("backdrop", e);
    // Fall back to our own background rather than a see-through window.
    if (kind !== "none") {
      document.documentElement.dataset.backdrop = "none";
      backdropSelect.value = "none";
    }
  }
}

backdropSelect.addEventListener("change", () => applyBackdrop(backdropSelect.value as Backdrop));
applyBackdrop(readBackdrop());

// ---- icons ----

const icons = new Map<string, Promise<string | null>>();

function iconFor(path: string) {
  let icon = icons.get(path);
  if (!icon) {
    icon = invoke<string | null>("app_icon", { path }).catch(() => null);
    icons.set(path, icon);
  }
  return icon;
}

// ---- drag and drop ----

function assign(processName: string, assignment: string) {
  invoke("assign_app", { processName, assignment });
}

/** `target` is a channel id or "none". */
function makeDropZone(zone: HTMLElement, target: string) {
  zone.dataset.target = target;
  zone.addEventListener("dragover", (e) => {
    if (!e.dataTransfer?.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    zone.classList.add("over");
  });
  zone.addEventListener("dragleave", (e) => {
    if (!zone.contains(e.relatedTarget as Node | null)) zone.classList.remove("over");
  });
  zone.addEventListener("drop", (e) => {
    e.preventDefault();
    zone.classList.remove("over");
    const processName = e.dataTransfer?.getData(DRAG_TYPE);
    if (processName) assign(processName, target);
  });
}

makeDropZone(uncontrolledEl, "none");

// ---- chip menu (click / keyboard alternative to dragging) ----

function openMenu(chip: HTMLElement, app: AppGroup) {
  if (!latest) return;
  const playback = latest.channels.filter((c) => c.kind === "playback");
  const options: [string, string, string?][] = [
    ...playback.map((c): [string, string, string] => [c.id, c.name, c.color]),
    ["none", tr().uncontrolled],
    ["default", tr().automatic],
  ];

  menuEl.replaceChildren(
    el("div", "menu-title", app.displayName),
    ...options.map(([value, label, color]) => {
      const item = el("button", "menu-item", label);
      item.type = "button";
      item.setAttribute("role", "menuitemradio");
      item.setAttribute("aria-checked", String(value === app.assignment));
      if (color) item.style.setProperty("--accent", color);
      item.addEventListener("click", () => {
        closeMenu();
        // The chip is rebuilt in its new place; keep keyboard focus on it.
        refocusProcess = app.processName;
        assign(app.processName, value);
      });
      return item;
    }),
  );

  const rect = chip.getBoundingClientRect();
  menuEl.hidden = false;
  const left = Math.min(rect.left, window.innerWidth - menuEl.offsetWidth - 8);
  const below = rect.bottom + 6;
  const top = below + menuEl.offsetHeight > window.innerHeight ? rect.top - menuEl.offsetHeight - 6 : below;
  menuEl.style.left = `${Math.max(8, left)}px`;
  menuEl.style.top = `${Math.max(8, top)}px`;
  const start =
    menuEl.querySelector<HTMLButtonElement>("[aria-checked=true]") ??
    menuEl.querySelector<HTMLButtonElement>(".menu-item");
  start?.focus();
  menuReturnFocus = chip;
}

let menuReturnFocus: HTMLElement | null = null;
/** Process whose chip should get focus after the next rebuild. */
let refocusProcess: string | null = null;

function closeMenu() {
  if (menuEl.hidden) return;
  menuEl.hidden = true;
  menuReturnFocus?.focus();
  menuReturnFocus = null;
}

document.addEventListener("pointerdown", (e) => {
  if (!menuEl.hidden && !menuEl.contains(e.target as Node)) closeMenu();
});
menuEl.addEventListener("keydown", (e) => {
  const items = [...menuEl.querySelectorAll<HTMLButtonElement>(".menu-item")];
  const index = items.indexOf(document.activeElement as HTMLButtonElement);
  if (e.key === "Escape") closeMenu();
  else if (e.key === "ArrowDown") items[(index + 1) % items.length]?.focus();
  else if (e.key === "ArrowUp") items[(index - 1 + items.length) % items.length]?.focus();
  else return;
  e.preventDefault();
});
window.addEventListener("blur", closeMenu);

// ---- chips ----

function buildChip(app: AppGroup, color: string | null) {
  const chip = el("div", "chip");
  chip.dataset.process = app.processName;
  chip.draggable = true;
  chip.tabIndex = 0;
  chip.setAttribute("role", "button");
  chip.setAttribute("aria-haspopup", "menu");
  chip.title = `${app.displayName} – ${app.processName}\n${tr().playsOn(app.devices.join(", "))}`;
  if (color) chip.style.setProperty("--accent", color);

  const icon = el("span", "chip-icon", app.displayName.slice(0, 1).toUpperCase());
  if (app.exePath) {
    iconFor(app.exePath).then((src) => {
      if (!src) return;
      const img = el("img");
      img.src = src;
      img.alt = "";
      icon.replaceChildren(img);
      icon.classList.add("has-image");
    });
  }
  chip.append(icon, el("span", "chip-name", app.displayName));
  if (app.assignment === "default") {
    const badge = el("span", "chip-badge", "auto");
    badge.title = tr().autoBadge;
    chip.append(badge);
  }

  chip.addEventListener("dragstart", (e) => {
    dragging = true;
    closeMenu();
    e.dataTransfer?.setData(DRAG_TYPE, app.processName);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
    chip.classList.add("lifted");
  });
  chip.addEventListener("dragend", () => {
    dragging = false;
    chip.classList.remove("lifted");
    document.querySelectorAll(".over").forEach((z) => z.classList.remove("over"));
    // Catch up on anything that arrived during the drag.
    if (latest) renderApps(latest, true);
  });
  chip.addEventListener("click", () => openMenu(chip, app));
  chip.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openMenu(chip, app);
    }
  });
  return chip;
}

function groupApps(sessions: SessionSnapshot[]): AppGroup[] {
  const byProcess = new Map<string, AppGroup>();
  for (const s of sessions) {
    if (!s.processName) continue; // system sounds can't be moved
    const key = s.processName.toLowerCase();
    const group =
      byProcess.get(key) ??
      ({
        processName: s.processName,
        displayName: s.displayName,
        exePath: s.exePath,
        assignment: s.assignment,
        channel: null,
        devices: [],
      } satisfies AppGroup);
    group.channel ??= s.channel;
    if (!group.devices.includes(s.deviceName)) group.devices.push(s.deviceName);
    byProcess.set(key, group);
  }
  return [...byProcess.values()];
}

function renderApps(snapshot: MixerSnapshot, force = false) {
  if (!force && snapshot.sessionsVersion === sessionsVersion) return;
  if (dragging) return;
  sessionsVersion = snapshot.sessionsVersion;

  const apps = groupApps(snapshot.sessions);
  const colorOf = (id: string | null) => snapshot.channels.find((c) => c.id === id)?.color ?? null;

  for (const [id, view] of channelViews) {
    if (!view.chips) continue;
    const chips = apps.filter((a) => a.channel === id).map((a) => buildChip(a, colorOf(id)));
    view.chips.replaceChildren(...(chips.length ? chips : [el("span", "empty", tr().dropHere)]));
  }
  const loose = apps.filter((a) => a.channel === null).map((a) => buildChip(a, null));
  uncontrolledEl.replaceChildren(...(loose.length ? loose : [el("span", "empty", tr().none)]));

  if (refocusProcess) {
    const wanted = refocusProcess;
    refocusProcess = null;
    [...document.querySelectorAll<HTMLElement>(".chip")].find((c) => c.dataset.process === wanted)?.focus();
  }
  renderDetails(snapshot);
}

function renderDetails(snapshot: MixerSnapshot) {
  const nameOf = (id: string | null) =>
    id ? (snapshot.channels.find((c) => c.id === id)?.name ?? id) : tr().uncontrolled;
  rowsEl.replaceChildren(
    ...groupApps(snapshot.sessions).map((app) => {
      const row = el("tr");
      const name = el("td", "app", app.displayName);
      name.title = app.exePath ?? "";
      const how = app.assignment === "default" ? `${nameOf(app.channel)} (auto)` : nameOf(app.channel);
      row.append(name, el("td", "muted-text", app.processName), el("td", "muted-text", app.devices.join(", ")), el("td", undefined, how));
      return row;
    }),
  );
}

// ---- channels ----

function buildChannel(ch: ChannelSnapshot): ChannelView {
  const root = el("article", "channel glass");
  root.style.setProperty("--accent", ch.color);

  const head = el("div", "channel-head");
  head.append(el("span", "dot"), el("h2", undefined, ch.name));

  const devicePill = el("label", "select-pill device-pill");
  const device = el("select", "device");
  device.setAttribute("aria-label", ch.kind === "capture" ? tr().micAria(ch.name) : tr().outputAria(ch.name));
  device.addEventListener("change", () => {
    const value = device.value || null;
    if (ch.kind === "capture") invoke("set_capture_device", { name: value });
    else invoke("set_channel_output", { channel: ch.id, device: value });
  });
  devicePill.append(device);

  // Fader, meter, and a side column with the steps, the value and mute.
  const body = el("div", "body");
  const fader = el("div", "fader");
  fader.tabIndex = 0;
  fader.setAttribute("role", "slider");
  fader.setAttribute("aria-label", tr().volumeAria(ch.name));
  fader.setAttribute("aria-valuemin", "0");
  fader.setAttribute("aria-valuemax", "100");
  const faderFill = el("div", "fader-fill");
  faderFill.append(el("span", "grip"));
  fader.append(faderFill);

  const meterTrack = el("div", "meter");
  const meter = el("div", "meter-fill");
  meterTrack.append(meter);

  const side = el("div", "side");
  const steps = el("div", "steps");
  const up = el("button", "round", "+");
  const down = el("button", "round", "−");
  up.setAttribute("aria-label", tr().stepUpAria(ch.name));
  down.setAttribute("aria-label", tr().stepDownAria(ch.name));
  up.title = tr().stepUpTitle;
  down.title = tr().stepDownTitle;
  steps.append(up, down);
  const value = el("div", "value");
  const mute = el("button", "mute", tr().mute);
  side.append(steps, value, mute);
  body.append(fader, meterTrack, side);

  root.append(head, devicePill, body);
  let chips: HTMLElement | null = null;
  let micInfo: HTMLElement | null = null;
  if (ch.kind === "playback") {
    chips = el("div", "dropzone chips");
    makeDropZone(chips, ch.id);
    root.append(chips);
  } else {
    const zone = el("div", "chips static");
    micInfo = el("div", "chip info");
    zone.append(micInfo);
    root.append(zone);
  }

  const view: ChannelView = {
    root,
    device,
    deviceKey: "",
    value,
    fader,
    faderFill,
    meter,
    mute,
    chips,
    micInfo,
    lastUserInput: -Infinity,
  };

  const setLocal = (volume: number) => {
    view.lastUserInput = performance.now();
    showVolume(view, volume);
    invoke("set_volume", { channel: ch.id, volume });
  };
  const fromPointer = (e: PointerEvent) => {
    const rect = fader.getBoundingClientRect();
    return Math.round(Math.max(0, Math.min(1, (rect.bottom - e.clientY) / rect.height)) * 100);
  };
  fader.addEventListener("pointerdown", (e) => {
    fader.setPointerCapture(e.pointerId);
    fader.focus();
    setLocal(fromPointer(e));
  });
  fader.addEventListener("pointermove", (e) => {
    if (fader.hasPointerCapture(e.pointerId)) setLocal(fromPointer(e));
  });
  fader.addEventListener("keydown", (e) => {
    const current = Number(fader.getAttribute("aria-valuenow") ?? 0);
    const step = (steps: number) => invoke("adjust_volume", { channel: ch.id, steps });
    if (e.key === "ArrowUp" || e.key === "ArrowRight") step(1);
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") step(-1);
    else if (e.key === "PageUp") setLocal(Math.min(100, current + 10));
    else if (e.key === "PageDown") setLocal(Math.max(0, current - 10));
    else if (e.key === "Home") setLocal(0);
    else if (e.key === "End") setLocal(100);
    else return;
    e.preventDefault();
  });

  up.addEventListener("click", () => invoke("adjust_volume", { channel: ch.id, steps: 1 }));
  down.addEventListener("click", () => invoke("adjust_volume", { channel: ch.id, steps: -1 }));
  mute.addEventListener("click", () => invoke("toggle_mute", { channel: ch.id }));
  return view;
}

function showVolume(view: ChannelView, volume: number) {
  view.value.textContent = `${volume}%`;
  view.faderFill.style.height = `${volume}%`;
  view.fader.setAttribute("aria-valuenow", String(volume));
  view.fader.setAttribute("aria-valuetext", tr().percent(volume));
}
/** Fills a device picker; rebuilt only when the choices change. */
function renderDevicePicker(view: ChannelView, ch: ChannelSnapshot, snapshot: MixerSnapshot) {
  const capture = ch.kind === "capture";
  const devices = capture ? snapshot.captureDevices : snapshot.outputDevices;
  const chosen = capture ? snapshot.captureDevice : ch.output;
  const missing = capture ? !!chosen && !devices.includes(chosen) : !ch.outputAvailable;
  const hidden = !capture && !snapshot.routingSupported;

  const key = JSON.stringify([devices, chosen, missing, hidden]);
  if (key === view.deviceKey) return;
  view.deviceKey = key;

  const pill = view.device.parentElement as HTMLElement;
  pill.hidden = hidden;
  pill.classList.toggle("missing", missing);
  view.device.title = hidden ? "" : capture ? tr().micPicker : tr().outputPicker;
  const options: [string, string][] = [["", tr().windowsDefault], ...devices.map((d): [string, string] => [d, d])];
  // Keep a saved choice visible while the device is unplugged.
  if (chosen && missing) options.push([chosen, tr().notConnected(chosen)]);
  view.device.replaceChildren(
    ...options.map(([value, label]) => {
      const option = el("option", undefined, label);
      option.value = value;
      return option;
    }),
  );
  view.device.value = chosen ?? "";
}

function renderChannels(snapshot: MixerSnapshot) {
  const channels = snapshot.channels;
  const order = channels.map((c) => c.id).join(",");
  if (order !== channelOrder) {
    channelOrder = order;
    channelViews.clear();
    channelsEl.replaceChildren(
      ...channels.map((c) => {
        const view = buildChannel(c);
        channelViews.set(c.id, view);
        return view.root;
      }),
    );
    sessionsVersion = -1; // new drop zones need their chips
  }

  for (const ch of channels) {
    const view = channelViews.get(ch.id)!;
    const touching = performance.now() - view.lastUserInput < USER_INPUT_GRACE_MS;
    if (!touching) showVolume(view, ch.volume);
    view.meter.style.height = `${meterLevel(ch.muted ? 0 : ch.peak) * 100}%`;
    view.root.classList.toggle("muted", ch.muted);
    view.root.classList.toggle("unavailable", !ch.available);
    view.mute.textContent = ch.muted ? tr().muted : tr().mute;
    view.mute.setAttribute("aria-pressed", String(ch.muted));
    if (view.micInfo) {
      const text = ch.available ? tr().micInput(ch.apps[0] ?? "") : tr().noMic;
      if (view.micInfo.textContent !== text) {
        view.micInfo.textContent = text;
        view.micInfo.title = text;
      }
    }
    renderDevicePicker(view, ch, snapshot);
  }
}

listen<MixerSnapshot>("mixer-snapshot", ({ payload }) => {
  latest = payload;
  applyLanguage(payload.language);
  renderChannels(payload);
  renderApps(payload);
});
