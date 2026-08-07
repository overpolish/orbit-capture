import { withThemeByClassName } from "@storybook/addon-themes";
import { themes } from "storybook/theming";

import type { Decorator, Preview } from "@storybook/react-vite";

import "../src/index.css";
import "./styles.css";

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
