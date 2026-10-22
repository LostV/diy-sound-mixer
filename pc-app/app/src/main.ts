import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ChannelSnapshot, MixerSnapshot, SessionSnapshot } from "./types";

/** Ignore incoming volumes this long after the user touched a slider, so it doesn't jump back. */
const USER_INPUT_GRACE_MS = 500;

interface ChannelView {
  root: HTMLElement;
  value: HTMLElement;
  slider: HTMLInputElement;
  meter: HTMLElement;
  mute: HTMLButtonElement;
  apps: HTMLElement;
  lastUserInput: number;
}

const channelViews = new Map<string, ChannelView>();
let channelOrder = "";
let sessionsVersion = -1;
let micKey = "";

const channelsEl = document.querySelector<HTMLElement>("#channels")!;
const rowsEl = document.querySelector<HTMLElement>("#session-rows")!;
const micSelect = document.querySelector<HTMLSelectElement>("#mic-select")!;

micSelect.addEventListener("change", () => {
  invoke("set_capture_device", { name: micSelect.value || null });
});

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

function buildChannel(ch: ChannelSnapshot): ChannelView {
  const root = el("article", "channel");
  root.style.setProperty("--accent", ch.color);

  const title = el("h2", undefined, ch.name);
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
  const apps = el("ul", "apps");

  root.append(title, value, body, steps, mute, apps);

  const view: ChannelView = { root, value, slider, meter, mute, apps, lastUserInput: 0 };
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

function renderChannels(channels: ChannelSnapshot[]) {
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

    const appsText = ch.available ? ch.apps.join("\n") : "Ingen mikrofon";
    if (view.apps.dataset.text !== appsText) {
      view.apps.dataset.text = appsText;
      const items = ch.available ? ch.apps : ["Ingen mikrofon"];
      view.apps.replaceChildren(...(items.length ? items : ["–"]).map((a) => el("li", undefined, a)));
    }
  }
}

/** One row per program: assignment is per program, sessions are per program × device. */
function groupSessions(sessions: SessionSnapshot[]) {
  const byProcess = new Map<string, { first: SessionSnapshot; devices: Set<string>; channels: Set<string> }>();
  for (const s of sessions) {
    const key = s.processName.toLowerCase() || `system:${s.displayName}`;
    const group = byProcess.get(key) ?? { first: s, devices: new Set(), channels: new Set() };
    group.devices.add(s.deviceName);
    group.channels.add(s.channel ?? "");
    byProcess.set(key, group);
  }
  return [...byProcess.values()];
}

function renderSessions(snapshot: MixerSnapshot) {
  if (snapshot.sessionsVersion === sessionsVersion) return;
  sessionsVersion = snapshot.sessionsVersion;

  const playback = snapshot.channels.filter((c) => c.kind === "playback");
  const nameOf = (id: string) => (id ? (playback.find((c) => c.id === id)?.name ?? id) : "Ikke styrt");

  rowsEl.replaceChildren(
    ...groupSessions(snapshot.sessions).map(({ first, devices, channels }) => {
      const row = el("tr");
      const app = el("td", "app", first.displayName);
      app.title = first.exePath ?? "";
      const process = el("td", "muted-text", first.processName || "–");
      const device = el("td", "muted-text", [...devices].join(", "));
      const current = el("td", undefined, [...channels].map(nameOf).join(", "));

      const ruleCell = el("td");
      if (first.processName) {
        const select = el("select");
        const options: [string, string][] = [
          ["default", "Standard"],
          ...playback.map((c): [string, string] => [c.id, c.name]),
          ["none", "Ikke styrt"],
        ];
        for (const [value, label] of options) {
          const option = el("option", undefined, label);
          option.value = value;
          select.append(option);
        }
        select.value = first.assignment;
        select.addEventListener("change", () =>
          invoke("assign_app", { processName: first.processName, assignment: select.value }),
        );
        ruleCell.append(select);
      } else {
        ruleCell.append(el("span", "muted-text", "–"));
      }

      row.append(app, process, device, current, ruleCell);
      return row;
    }),
  );
}

function renderMicPicker(snapshot: MixerSnapshot) {
  const key = JSON.stringify([snapshot.captureDevices, snapshot.captureDevice]);
  if (key === micKey) return;
  micKey = key;

  const options = [["", "Windows-standard"], ...snapshot.captureDevices.map((d) => [d, d])];
  // Keep a saved preference visible even when that mic is unplugged.
  if (snapshot.captureDevice && !snapshot.captureDevices.includes(snapshot.captureDevice)) {
    options.push([snapshot.captureDevice, `${snapshot.captureDevice} (ikke tilkoblet)`]);
  }
  micSelect.replaceChildren(
    ...options.map(([value, label]) => {
      const option = el("option", undefined, label);
      option.value = value;
      return option;
    }),
  );
  micSelect.value = snapshot.captureDevice ?? "";
}

listen<MixerSnapshot>("mixer-snapshot", ({ payload }) => {
  renderChannels(payload.channels);
  renderSessions(payload);
  renderMicPicker(payload);
});
