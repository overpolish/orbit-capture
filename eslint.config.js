import eslintReact from "@eslint-react/eslint-plugin";
import eslint from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import eslintConfigPrettier from "eslint-config-prettier";
import importX from "eslint-plugin-import-x";
import perfectionist from "eslint-plugin-perfectionist";
import reactRefresh from "eslint-plugin-react-refresh";
import sortDestructureKeys from "eslint-plugin-sort-destructure-keys";
import globals from "globals";
import tseslint from "typescript-eslint";

const frontendFiles = [
  ".storybook/**/*.ts",
  "src/**/*.{ts,tsx}",
  "*.config.ts",
];

export default defineConfig([
  globalIgnores([
    "dist/**",
    "node_modules/**",
    "src-tauri/**",
    "storybook-static/**",
  ]),
  {
    extends: [eslint.configs.recommended],
    files: ["**/*.{js,mjs,cjs,ts,jsx,tsx}"],
    languageOptions: {
      globals: globals.browser,
    },
  },
  ...tseslint.configs.strictTypeChecked.map((config) => ({
    ...config,
    files: frontendFiles,
  })),
  {
    files: frontendFiles,
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/max-params": "error",
      "@typescript-eslint/member-ordering": [
        "error",
        {
          default: {
            memberTypes: ["field", "constructor", "method"],
            optionalityOrder: "required-first",
            order: "natural",
          },
        },
      ],
      "@typescript-eslint/no-unused-vars": [
        "warn",
        {
          argsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
        },
      ],
    },
  },
  {
    ...eslintReact.configs["recommended-type-checked"],
    files: ["src/**/*.{ts,tsx}"],
  },
  {
    ...importX.configs["flat/recommended"],
    files: frontendFiles,
    rules: {
      ...importX.configs["flat/recommended"].rules,
      "import-x/order": [
        "error",
        {
          alphabetize: {
            caseInsensitive: true,
            order: "asc",
          },
          groups: [
            "builtin",
            "external",
            "internal",
            "parent",
            "sibling",
            "index",
            "object",
            "type",
          ],
          "newlines-between": "always",
          pathGroups: [
            {
              group: "external",
              pattern: "react",
              position: "before",
            },
          ],
        },
      ],
    },
  },
  {
    ...importX.configs["flat/typescript"],
    files: frontendFiles,
  },
  {
    files: frontendFiles,
    plugins: {
      perfectionist,
      "sort-destructure-keys": sortDestructureKeys,
    },
    rules: {
      "no-restricted-exports": [
        "error",
        { restrictDefaultExports: { direct: true } },
      ],
      "perfectionist/sort-jsx-props": "error",
      "sort-destructure-keys/sort-destructure-keys": "error",
      "sort-keys": [
        "error",
        "asc",
        {
          allowLineSeparatedGroups: true,
          natural: true,
        },
      ],
    },
  },
  {
    ...reactRefresh.configs.vite,
    files: ["src/**/*.{ts,tsx}"],
  },
  {
    files: [".storybook/**/*.ts", "*.config.{js,ts}"],
    rules: {
      "no-restricted-exports": "off",
    },
  },
  {
    files: ["src/**/*.stories.{ts,tsx}"],
    rules: {
      "no-restricted-exports": "off",
    },
  },
  eslintConfigPrettier,
]);
