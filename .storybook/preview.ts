// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { withThemeByClassName } from "@storybook/addon-themes";
import { themes } from "storybook/theming";

import type { Decorator, Preview } from "@storybook/react-vite";

import "../src/index.css";
import "./styles.css";

// Initialize React Aria's focus tracking before Storybook's test loader wraps
// HTMLElement.prototype.focus. Lazy initialization after that wrapper is unsafe.
const [reactAria, storybookComponents] = await Promise.all([
  import("react-aria"),
  import("storybook/internal/components"),
]);

Object.freeze([reactAria.useOverlay, storybookComponents.Button]);

const preview: Preview = {
  parameters: {
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
    docs: {
      theme: themes.dark,
    },
  },
  tags: ["autodocs"],
};

export const decorators = [
  withThemeByClassName({
    defaultTheme: "dark",
    parentSelector: "html",
    themes: {
      dark: "dark",
      light: "light",
    },
  }),
] as Decorator[];

export default preview;
