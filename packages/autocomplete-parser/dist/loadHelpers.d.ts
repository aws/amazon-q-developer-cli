import { Logger } from "loglevel";
export type SpecFileImport = {
    default: Fig.Spec;
    getVersionCommand?: Fig.GetVersionCommand;
} | {
    default: Fig.Subcommand;
    versions: Fig.VersionDiffMap;
};
export declare const importString: (str: string) => Promise<any>;
export declare function importSpecFromFile(name: string, path: string, localLogger?: Logger): Promise<SpecFileImport>;
/**
 * Specs can only be loaded from non "secure" contexts, so we can't load from https
 */
export declare const canLoadSpecProtocol: () => boolean;
export declare function importFromPublicCDN<T = SpecFileImport>(name: string): Promise<T>;
export declare function importFromLocalhost<T = SpecFileImport>(name: string, port: number | string): Promise<T>;
export declare const getCachedCLIVersion: (key: string) => string | null;
export declare function getVersionFromFullFile(specData: SpecFileImport, name: string): Promise<string | undefined>;
export declare function clearSpecIndex(): void;
export declare function publicSpecExists(name: string): Promise<boolean>;
export declare function isDiffVersionedSpec(name: string): Promise<boolean>;
export declare function preloadSpecs(): Promise<SpecFileImport[]>;
