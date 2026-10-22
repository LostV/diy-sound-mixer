// Mirrors mixer-core's snapshot.rs (serde camelCase).

export type ChannelKind = "playback" | "capture";

export interface ChannelSnapshot {
  id: string;
  name: string;
  color: string;
  kind: ChannelKind;
  volume: number;
  muted: boolean;
  peak: number;
  available: boolean;
  apps: string[];
  /** Chosen output device name; null = Windows default. Playback only. */
  output: string | null;
  /** False when the chosen output is not connected right now. */
  outputAvailable: boolean;
}

export interface SessionSnapshot {
  id: string;
  processName: string;
  displayName: string;
  exePath: string | null;
  deviceName: string;
  channel: string | null;
  /** "default", "none" or a channel id */
  assignment: string;
}

export interface MixerSnapshot {
  channels: ChannelSnapshot[];
  sessions: SessionSnapshot[];
  sessionsVersion: number;
  captureDevices: string[];
  captureDevice: string | null;
  outputDevices: string[];
  routingSupported: boolean;
}
