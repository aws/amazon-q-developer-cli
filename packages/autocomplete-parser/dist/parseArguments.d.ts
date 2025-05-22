import logger from "loglevel";
import * as Internal from "@aws/amazon-q-developer-cli-shared/internal";
import { SuggestionFlags } from "@aws/amazon-q-developer-cli-shared/utils";
import { Command } from "@aws/amazon-q-developer-cli-shell-parser";
type ArgArrayState = {
    args: Array<Internal.Arg> | null;
    index: number;
    variadicCount?: number;
};
export declare enum TokenType {
    None = "none",
    Subcommand = "subcommand",
    Option = "option",
    OptionArg = "option_arg",
    SubcommandArg = "subcommand_arg",
    Composite = "composite"
}
export type BasicAnnotation = {
    text: string;
    type: Exclude<TokenType, TokenType.Composite>;
    tokenName?: string;
} | {
    text: string;
    type: TokenType.Subcommand;
    spec: Internal.Subcommand;
    specLocation: Internal.SpecLocation;
};
type CompositeAnnotation = {
    text: string;
    type: TokenType.Composite;
    subtokens: BasicAnnotation[];
};
export type Annotation = BasicAnnotation | CompositeAnnotation;
export type ArgumentParserState = {
    completionObj: Internal.Subcommand;
    optionArgState: ArgArrayState;
    subcommandArgState: ArgArrayState;
    annotations: Annotation[];
    passedOptions: Internal.Option[];
    commandIndex: number;
    haveEnteredSubcommandArgs: boolean;
    isEndOfOptions: boolean;
};
export type ArgumentParserResult = {
    completionObj: Internal.Subcommand;
    currentArg: Internal.Arg | null;
    passedOptions: Internal.Option[];
    searchTerm: string;
    commandIndex: number;
    suggestionFlags: SuggestionFlags;
    annotations: Annotation[];
};
export declare const createArgState: (args?: Internal.Arg[]) => ArgArrayState;
export declare const flattenAnnotations: (annotations: Annotation[]) => BasicAnnotation[];
export declare const optionsAreEqual: (a: Internal.Option, b: Internal.Option) => boolean;
export declare const countEqualOptions: (option: Internal.Option, options: Internal.Option[]) => number;
export declare const updateArgState: (argState: ArgArrayState) => ArgArrayState;
export declare const getCurrentArg: (argState: ArgArrayState) => Internal.Arg | null;
export declare const isMandatoryOrVariadic: (arg: Internal.Arg | null) => boolean;
export declare const findOption: (spec: Internal.Subcommand, token: string) => Internal.Option;
export declare const findSubcommand: (spec: Internal.Subcommand, token: string) => Internal.Subcommand;
export declare const getResultFromState: (state: ArgumentParserState) => ArgumentParserResult;
export declare const initialParserState: ArgumentParserResult;
export declare const clearFigCaches: () => {
    unsubscribe: boolean;
};
export declare const parseArguments: (command: Command | null, context: Fig.ShellContext, isParsingHistory?: boolean, localLogger?: logger.Logger) => Promise<ArgumentParserResult>;
export {};
