export declare enum SuggestionFlag {
    None = 0,
    Subcommands = 1,
    Options = 2,
    Args = 4,
    Any = 7
}
export type SuggestionFlags = number;
export declare enum SpecLocationSource {
    GLOBAL = "global",
    LOCAL = "local"
}
export declare function makeArray<T>(object: T | T[]): T[];
export declare function firstMatchingToken(str: string, chars: Set<string>): string | undefined;
export declare function makeArrayIfExists<T>(obj: T | T[] | null | undefined): T[] | null;
export declare function isOrHasValue(obj: string | Array<string>, valueToMatch: string): boolean;
export declare const TimeoutError: {
    new (message?: string): {
        name: string;
        message: string;
        stack?: string;
        cause?: unknown;
    };
};
export declare function withTimeout<T>(time: number, promise: Promise<T>): Promise<T>;
export declare const longestCommonPrefix: (strings: string[]) => string;
export declare function findLast<T>(values: T[], predicate: (v: T) => boolean): T | undefined;
type NamedObject = {
    name?: string[] | string;
} | string;
export declare function compareNamedObjectsAlphabetically<A extends NamedObject, B extends NamedObject>(a: A, b: B): number;
export declare const sleep: (ms: number) => Promise<void>;
export type Func<S extends unknown[], T> = (...args: S) => T;
type EqualFunc<T> = (args: T, newArgs: T) => boolean;
export declare function memoizeOne<S extends unknown[], T>(fn: Func<S, T>, isEqual?: EqualFunc<S>): Func<S, T>;
/**
 * If no fields are specified and A,B are not equal primitives/empty objects, this returns false
 * even if the objects are actually equal.
 */
export declare function fieldsAreEqual<T>(A: T, B: T, fields: (keyof T)[]): boolean;
export declare const splitPath: (path: string) => [string, string];
export declare const ensureTrailingSlash: (str: string) => string;
export declare const getCWDForFilesAndFolders: (cwd: string | null, searchTerm: string) => string;
export declare function localProtocol(domain: string, path: string): string;
type ExponentialBackoffOptions = {
    attemptTimeout: number;
    baseDelay: number;
    maxRetries: number;
    jitter: number;
};
export declare function exponentialBackoff<T>(options: ExponentialBackoffOptions, fn: () => Promise<T>): Promise<T>;
export {};
