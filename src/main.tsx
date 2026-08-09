// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "./index.css";
import { synchronizeSystemTheme } from "./lib/theme";

synchronizeSystemTheme();

if (navigator.userAgent.includes("Windows")) {
  document.documentElement.dataset.platform = "windows";
}

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
