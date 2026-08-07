import { Channel, invoke } from "@tauri-apps/api/core";

export type AudioPreviewKind = "microphone" | "system";

export type AudioPreviewEvent =
  | { data: { decibels: number }; event: "signal" }
  | { data: { message: string }; event: "error" };

type StartAudioPreviewOptions = {
  channel: Channel<AudioPreviewEvent>;
  kind: AudioPreviewKind;
  applicationIds?: string[];
  deviceId?: string;
  processIds?: number[];
};

export const startAudioPreview = async ({
  applicationIds,
  channel,
  deviceId,
  kind,
  processIds,
}: StartAudioPreviewOptions) => {
  await invoke("start_audio_preview", {
    applicationIds,
    channel,
    deviceId,
    kind,
    processIds,
  });
};

export const stopAudioPreview = async (kind: AudioPreviewKind) => {
  await invoke("stop_audio_preview", { kind });
};
