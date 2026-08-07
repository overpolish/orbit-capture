export type MonitorDetails = {
  id: number;
  isBuiltin: boolean;
  isPrimary: boolean;
  name: string;
  position: { x: number; y: number };
  scaleFactor: number;
  size: { height: number; width: number };
};

export type RecordingMode = "screen" | "region" | "window" | "audio";

export type SelectorPlacement = "above" | "below";

export type WindowDetails = {
  appIconPath: string | null;
  appName: string;
  id: number;
  pid: number;
  position: { x: number; y: number };
  size: { height: number; width: number };
  thumbnailPath: string | null;
  title: string;
};
