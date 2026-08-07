export type MonitorDetails = {
  id: number;
  isBuiltin: boolean;
  isPrimary: boolean;
  name: string;
  position: { x: number; y: number };
  scaleFactor: number;
  size: { height: number; width: number };
};

export type SelectorPlacement = "above" | "below";
