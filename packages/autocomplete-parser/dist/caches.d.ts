import { Subcommand } from "@aws/amazon-q-developer-cli-shared/internal";
export declare const createCache: <T>() => Map<string, T>;
export declare const resetCaches: () => void;
export declare const specCache: Map<string, Subcommand>;
export declare const generateSpecCache: Map<string, Subcommand>;
