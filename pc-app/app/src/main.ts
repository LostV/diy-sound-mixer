import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ChannelSnapshot, MixerSnapshot, SessionSnapshot } from "./types";

/** Ignore incoming volumes this long after the user touched a slider, so it doesn't jump back. */
const USER_INPUT_GRACE_MS = 500;
const DRAG_TYPE = "application/x-diy-mixer-process";

interface ChannelView {
  root: HTMLElement;
  device: HTMLSelectElement;
  deviceKey: string;
  value: HTMLElement;
  slider: HTMLInputElement;
  meter: HTMLElement;
  mute: HTMLButtonElement;
  /** Drop zone with the app chips; null for the mic. */
  chips: HTMLElement | null;
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

document.querySelector("#open-simulator")!.addEventListener("click", () => invoke("show_simulator"));

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
    ["none", "Ikke styrt"],
    ["default", "Automatisk (regler)"],
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
  chip.title = `${app.displayName} – ${app.processName}\nSpiller på: ${app.devices.join(", ")}`;
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
    badge.title = "Plassert automatisk av reglene. Dra eller velg en kanal for å låse den.";
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
    view.chips.replaceChildren(...(chips.length ? chips : [el("span", "empty", "Dra apper hit")]));
  }
  const loose = apps.filter((a) => a.channel === null).map((a) => buildChip(a, null));
  uncontrolledEl.replaceChildren(...(loose.length ? loose : [el("span", "empty", "Ingen")]));

  if (refocusProcess) {
    const wanted = refocusProcess;
    refocusProcess = null;
    [...document.querySelectorAll<HTMLElement>(".chip")].find((c) => c.dataset.process === wanted)?.focus();
  }
  renderDetails(snapshot);
}

function renderDetails(snapshot: MixerSnapshot) {
  const nameOf = (id: string | null) =>
    id ? (snapshot.channels.find((c) => c.id === id)?.name ?? id) : "Ikke styrt";
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
  const root = el("article", "channel");
  root.style.setProperty("--accent", ch.color);

  const title = el("h2", undefined, ch.name);
  const device = el("select", "device");
  device.setAttribute("aria-label", ch.kind === "capture" ? `${ch.name}: mikrofon` : `${ch.name}: utgang`);
  device.addEventListener("change", () => {
    const value = device.value || null;
    if (ch.kind === "capture") invoke("set_capture_device", { name: value });
    else invoke("set_channel_output", { channel: ch.id, device: value });
  });

  const value = el("div", "value");

  const body = el("div", "body");
  const slider = el("input", "slider");
  slider.type = "range";
  slider.min = "0";
  slider.max = "100";
  slider.setAttribute("aria-label", `${ch.name} volum`);
  const meterTrack = el("div", "meter");
  const meter = el("div", "meter-fill");
  meterTrack.append(meter);
  body.append(slider, meterTrack);

  const steps = el("div", "steps");
  const down = el("button", "step", "−");
  const up = el("button", "step", "+");
  down.title = "Ett klikk mot klokka (som knotten)";
  up.title = "Ett klikk med klokka (som knotten)";
  steps.append(down, up);

  const mute = el("button", "mute", "Mute");

  let chips: HTMLElement | null = null;
  root.append(title, device, value, body, steps, mute);
  if (ch.kind === "playback") {
    chips = el("div", "dropzone chips");
    makeDropZone(chips, ch.id);
    root.append(chips);
  }

  const view: ChannelView = { root, device, deviceKey: "", value, slider, meter, mute, chips, lastUserInput: -Infinity };
  slider.addEventListener("input", () => {
    view.lastUserInput = performance.now();
    view.value.textContent = `${slider.value}%`;
    invoke("set_volume", { channel: ch.id, volume: Number(slider.value) });
  });
  down.addEventListener("click", () => invoke("adjust_volume", { channel: ch.id, steps: -1 }));
  up.addEventListener("click", () => invoke("adjust_volume", { channel: ch.id, steps: 1 }));
  mute.addEventListener("click", () => invoke("toggle_mute", { channel: ch.id }));
  return view;
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

  view.device.hidden = hidden;
  view.device.title = hidden ? "" : capture ? "Mikrofon for denne kanalen" : "Hvor appene i kanalen spiller";
  const options: [string, string][] = [["", "Windows-standard"], ...devices.map((d): [string, string] => [d, d])];
  // Keep a saved choice visible while the device is unplugged.
  if (chosen && missing) options.push([chosen, `${chosen} (ikke tilkoblet)`]);
  view.device.replaceChildren(
    ...options.map(([value, label]) => {
      const option = el("option", undefined, label);
      option.value = value;
      return option;
    }),
  );
  view.device.value = chosen ?? "";
  view.device.classList.toggle("missing", missing);
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
    if (!touching) {
      view.slider.value = String(ch.volume);
      view.value.textContent = `${ch.volume}%`;
    }
    view.meter.style.height = `${meterLevel(ch.muted ? 0 : ch.peak) * 100}%`;
    view.root.classList.toggle("muted", ch.muted);
    view.root.classList.toggle("unavailable", !ch.available);
    view.mute.textContent = ch.muted ? "Mutet" : "Mute";
    view.mute.setAttribute("aria-pressed", String(ch.muted));
    renderDevicePicker(view, ch, snapshot);
  }
}

listen<MixerSnapshot>("mixer-snapshot", ({ payload }) => {
  latest = payload;
  renderChannels(payload);
  renderApps(payload);
});
