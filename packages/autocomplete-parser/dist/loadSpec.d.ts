import { Logger } from "loglevel";
import { Subcommand, SpecLocation } from "@aws/amazon-q-developer-cli-shared/internal";
import { SpecFileImport } from "./loadHelpers.js";
export declare const serializeSpecLocation: (location: SpecLocation) => string;
export declare const getSpecPath: (name: string, cwd: string, isScript?: boolean) => Promise<SpecLocation>;
type ResolvedSpecLocation = {
    type: "public";
    name: string;
} | {
    type: "private";
    namespace: string;
    name: string;
};
export declare const importSpecFromLocation: (specLocation: SpecLocation, localLogger?: Logger) => Promise<{
    specFile: SpecFileImport;
    resolvedLocation?: ResolvedSpecLocation;
}>;
export declare const loadFigSubcommand: (specLocation: SpecLocation, _context?: Fig.ShellContext, localLogger?: Logger) => Promise<Fig.Subcommand>;
export declare const loadSubcommandCached: (specLocation: SpecLocation, context?: Fig.ShellContext, localLogger?: Logger) => Promise<Subcommand>;
export {};
