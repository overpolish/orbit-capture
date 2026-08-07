import { create } from "zustand";

import {
  initialPermissionSnapshot,
  PermissionKind,
  PermissionSnapshot,
} from "./types";

type PermissionStore = {
  hydrated: boolean;
  permissions: PermissionSnapshot;
  setPermissions: (permissions: PermissionSnapshot) => void;
};

export const usePermissionStore = create<PermissionStore>()((set) => ({
  hydrated: false,
  permissions: initialPermissionSnapshot,
  setPermissions: (permissions) => {
    set({ hydrated: true, permissions });
  },
}));

export const selectPermission =
  (permission: PermissionKind) => (state: PermissionStore) =>
    state.permissions[permission];

export const selectCanRecordScreen = (state: PermissionStore) =>
  state.permissions.accessibility.granted &&
  state.permissions.screenRecording.granted;

export const selectCanRecordCamera = (state: PermissionStore) =>
  state.permissions.camera.granted;

export const selectCanRecordMicrophone = (state: PermissionStore) =>
  state.permissions.microphone.granted;
