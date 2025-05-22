import tseslint from "typescript-eslint";
import { CONFIG } from "./common.js";
const config = ({ tsconfigPath, }) => tseslint.config(...CONFIG, {
    languageOptions: {
        parserOptions: {
            project: tsconfigPath,
        },
    },
    ignores: ["*.config.{js,ts}"],
});
export default config;
//# sourceMappingURL=base.config.js.map