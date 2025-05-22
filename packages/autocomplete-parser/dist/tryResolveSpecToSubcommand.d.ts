import { SpecLocation } from "@aws/amazon-q-developer-cli-shared/internal";
import { SpecFileImport } from "./loadHelpers.js";
export declare const tryResolveSpecToSubcommand: (spec: SpecFileImport, location: SpecLocation) => Promise<Fig.Subcommand>;
