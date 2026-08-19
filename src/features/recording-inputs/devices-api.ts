// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";

import { CameraDevice, InputDevice, SystemAudioSource } from "./types";

type ApplicationDetails = {
  iconPath: string | null;
  id: string;
  label: string;
  processIds: number[];
};

export const listCameras = (preferredFps: number[]) =>
  invoke<CameraDevice[]>("list_cameras", { preferredFps });

export const listMicrophones = () => invoke<InputDevice[]>("list_microphones");

export const listSystemAudioSources = async (): Promise<
  SystemAudioSource[]
> => {
  const applications = await invoke<ApplicationDetails[]>("list_applications");
  return applications.map((application) => ({
    ...application,
    kind: "application",
  }));
};
