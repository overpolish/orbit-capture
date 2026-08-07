import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  addons: ["@storybook/addon-docs", "@storybook/addon-themes"],
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  stories: ["../src/**/*.stories.@(js|jsx|mjs|ts|tsx)"],
  viteFinal: (viteConfig) => {
    viteConfig.server ??= {};
    viteConfig.server.watch = {
      ...viteConfig.server.watch,
      ignored: ["**/dist/**", "**/src-tauri/**", "**/storybook-static/**"],
    };

    return viteConfig;
  },
};

export default config;
