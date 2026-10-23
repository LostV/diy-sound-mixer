// UI texts in English and Norwegian. The language itself lives in the mixer
// config (it also drives the tray and the screen); windows get it from the
// snapshot or, in the simulator, from the protocol's `lang` message.

export type Language = "en" | "no";

const en = {
  // main window
  autostart: "Start with Windows",
  language: "Language",
  settings: "Settings",
  backdrop: "Background",
  backdropTitle: "Window background",
  backdropSolid: "Solid",
  minimize: "Minimize",
  closeToTray: "Close to the system tray",
  channels: "Channels",
  uncontrolled: "Not controlled",
  uncontrolledHint: "Drag an app here to leave it alone · click an app for more options",
  details: "Details",
  colApp: "App",
  colProcess: "Process",
  colPlaysOn: "Plays on",
  colPlaced: "Placed",
  automatic: "Automatic (rules)",
  playsOn: (devices: string) => `Plays on: ${devices}`,
  autoBadge: "Placed automatically by the rules. Drag it or pick a channel to lock it.",
  dropHere: "Drag apps here",
  none: "None",
  micAria: (channel: string) => `${channel}: microphone`,
  outputAria: (channel: string) => `${channel}: output`,
  volumeAria: (channel: string) => `${channel} volume`,
  stepUpAria: (channel: string) => `${channel}: up one step`,
  stepDownAria: (channel: string) => `${channel}: down one step`,
  stepUpTitle: "One click clockwise (like the knob)",
  stepDownTitle: "One click counterclockwise (like the knob)",
  mute: "Mute",
  muted: "Muted",
  percent: (value: number) => `${value} percent`,
  micInput: (device: string) => `Input · ${device}`,
  noMic: "No microphone",
  micPicker: "Microphone for this channel",
  outputPicker: "Where the apps in this channel play",
  windowsDefault: "Windows default",
  notConnected: (device: string) => `${device} (not connected)`,
  // simulator
  simDevice: "Simulated mixer",
  simScreen: "Screen 800×480",
  protocol: "Protocol",
  showMeter: "Show meter",
  clear: "Clear",
  simHint:
    "Turn: mouse wheel or drag up/down. Click the knob or the screen to mute. Arrow keys work when a knob has focus. " +
    "Everything goes through the same protocol as the real hardware.",
  logIn: "PC → device",
  logOut: "device → PC",
  invalid: "invalid",
  columnAria: (channel: string, volume: number, muted: boolean) =>
    `${channel} ${volume} percent${muted ? ", muted" : ""}. Press to mute.`,
  noDevice: "No device",
  knob: (n: number) => `Knob ${n}`,
  muteKey: (n: number) => `Mute button ${n}`,
};

export type Strings = typeof en;

const no: Strings = {
  autostart: "Start med Windows",
  language: "Språk",
  settings: "Innstillinger",
  backdrop: "Bakgrunn",
  backdropTitle: "Vindusbakgrunn",
  backdropSolid: "Ensfarget",
  minimize: "Minimer",
  closeToTray: "Lukk til systemstatusfeltet",
  channels: "Kanaler",
  uncontrolled: "Ikke styrt",
  uncontrolledHint: "Dra en app hit for å la den være i fred · klikk på en app for flere valg",
  details: "Detaljer",
  colApp: "App",
  colProcess: "Prosess",
  colPlaysOn: "Spiller på",
  colPlaced: "Plassert",
  automatic: "Automatisk (regler)",
  playsOn: (devices) => `Spiller på: ${devices}`,
  autoBadge: "Plassert automatisk av reglene. Dra eller velg en kanal for å låse den.",
  dropHere: "Dra apper hit",
  none: "Ingen",
  micAria: (channel) => `${channel}: mikrofon`,
  outputAria: (channel) => `${channel}: utgang`,
  volumeAria: (channel) => `${channel} volum`,
  stepUpAria: (channel) => `${channel}: opp ett hakk`,
  stepDownAria: (channel) => `${channel}: ned ett hakk`,
  stepUpTitle: "Ett klikk med klokka (som knotten)",
  stepDownTitle: "Ett klikk mot klokka (som knotten)",
  mute: "Mute",
  muted: "Mutet",
  percent: (value) => `${value} prosent`,
  micInput: (device) => `Inngang · ${device}`,
  noMic: "Ingen mikrofon",
  micPicker: "Mikrofon for denne kanalen",
  outputPicker: "Hvor appene i kanalen spiller",
  windowsDefault: "Windows-standard",
  notConnected: (device) => `${device} (ikke tilkoblet)`,
  simDevice: "Simulert mikser",
  simScreen: "Skjerm 800×480",
  protocol: "Protokoll",
  showMeter: "Vis meter",
  clear: "Tøm",
  simHint:
    "Vri: musehjul eller dra opp/ned. Trykk på knotten eller skjermen = mute. Piltaster virker når knotten har fokus. " +
    "Alt går gjennom den samme protokollen som den ekte hardwaren.",
  logIn: "PC → enhet",
  logOut: "enhet → PC",
  invalid: "ugyldig",
  columnAria: (channel, volume, muted) => `${channel} ${volume} prosent${muted ? ", mutet" : ""}. Trykk for mute.`,
  noDevice: "Ingen enhet",
  knob: (n) => `Knott ${n}`,
  muteKey: (n) => `Mute-knapp ${n}`,
};

const tables: Record<Language, Strings> = { en, no };

/** Until the mixer says otherwise, guess from the WebView's (= Windows') language. */
let current: Language = /^(nb|nn|no)\b/i.test(navigator.language) ? "no" : "en";

export function language(): Language {
  return current;
}

export function tr(): Strings {
  return tables[current];
}

type TextKey = { [K in keyof Strings]: Strings[K] extends string ? K : never }[keyof Strings];

/**
 * Switches language and retranslates static markup: `data-i18n` sets the
 * text, `data-i18n-title` the tooltip and `data-i18n-label` the aria-label.
 * Returns false when nothing changed.
 */
export function setLanguage(lang: Language): boolean {
  const changed = lang !== current;
  current = lang;
  document.documentElement.lang = lang === "no" ? "nb" : "en";
  const strings = tr();
  const text = (key: string | undefined) => (key && key in strings ? (strings[key as TextKey] as string) : null);
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    const value = text(node.dataset.i18n);
    if (value !== null) node.textContent = value;
  }
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n-title]")) {
    const value = text(node.dataset.i18nTitle);
    if (value !== null) node.title = value;
  }
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n-label]")) {
    const value = text(node.dataset.i18nLabel);
    if (value !== null) node.setAttribute("aria-label", value);
  }
  return changed;
}
