import { BaseNode } from "./parser.js";
export * from "./errors.js";
export type Token = {
    text: string;
    node: BaseNode;
    originalNode: BaseNode;
};
export type Command = {
    tokens: Token[];
    tree: BaseNode;
    originalTree: BaseNode;
};
export type AliasMap = Record<string, string>;
export declare const createTextToken: (command: Command, index: number, text: string, originalNode?: BaseNode) => Token;
export declare const substituteAlias: (command: Command, token: Token, alias: string) => Command;
export declare const expandCommand: (command: Command, _cursorIndex: number, aliases: AliasMap) => Command;
export declare const getCommand: (buffer: string, aliases: AliasMap, cursorIndex?: number) => Command | null;
export declare const getTopLevelCommands: (parseTree: BaseNode) => Command[];
export declare const getAllCommandsWithAlias: (buffer: string, aliases: AliasMap) => Command[];
