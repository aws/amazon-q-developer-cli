import type { TSESLint } from "@typescript-eslint/utils";
declare const config: ({ tsconfigPath, }: {
    tsconfigPath: string;
}) => TSESLint.FlatConfig.ConfigArray;
export default config;
