export type InputDevice = {
  id: string;
  label: string;
  isDefault?: boolean;
};

export type SystemAudioSource = InputDevice & {
  kind: "all" | "application";
  iconPath?: string | null;
  processIds?: number[];
};

export type RecordingInputs = {
  camera: boolean;
  microphone: boolean;
  showCursor: boolean;
  systemAudio: boolean;
};
