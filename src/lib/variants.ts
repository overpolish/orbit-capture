import { createTV } from "tailwind-variants";
import { TVConfig } from "tailwind-variants/dist/config.js";

const twMergeConfig: TVConfig["twMergeConfig"] = {
  extend: {
    classGroups: {
      "font-size": ["text-xxs"],
    },
  },
};

export const tv = createTV({ twMergeConfig });
