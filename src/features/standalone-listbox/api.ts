import { invoke } from "@tauri-apps/api/core";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";

export const showStandaloneListbox = (
  parentWindowLabel: string,
  offset: LogicalPosition,
  size: LogicalSize,
) =>
  invoke<null>("show_standalone_listbox", {
    offset,
    parentWindowLabel,
    size,
  });

export const hideStandaloneListbox = () =>
  invoke<null>("hide_standalone_listbox");
