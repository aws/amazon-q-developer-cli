// @ts-check

import tseslint from "typescript-eslint";
import { CONFIG } from "./common.js";
// @ts-ignore
import react from "eslint-plugin-react/configs/recommended.js";
// @ts-ignore
import jsxRuntime from "eslint-plugin-react/configs/jsx-runtime.js";
// @ts-ignore
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import type { TSESLint } from "@typescript-eslint/utils";

const config = ({
  tsconfigPath,
}: {
  tsconfigPath: string;
}): TSESLint.FlatConfig.ConfigArray =>
  tseslint.config(
    ...CONFIG,
    react,
    jsxRuntime,
    {
      settings: {
        react: {
          version: "detect",
        },
      },
    },
    {
      plugins: {
        "react-hooks": reactHooks,
      },
      // @ts-ignore
      rules: reactHooks.configs.recommended.rules,
    },
    reactRefresh.configs.recommended,
    {
      languageOptions: {
        parserOptions: {
          project: tsconfigPath,
        },
      },
      ignores: ["*.config.{js,ts}"],
    },
  );

export default config;
