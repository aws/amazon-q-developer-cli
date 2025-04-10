import tseslint from "typescript-eslint";
import type { TSESLint } from "@typescript-eslint/utils";
import { CONFIG } from "./common.js";

const config = ({
  tsconfigPath,
}: {
  tsconfigPath: string;
}): TSESLint.FlatConfig.ConfigArray =>
  tseslint.config(...CONFIG, {
    languageOptions: {
      parserOptions: {
        project: tsconfigPath,
      },
    },
    ignores: ["*.config.{js,ts}"],
  });

export default config;
