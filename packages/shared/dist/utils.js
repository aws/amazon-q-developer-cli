import { createErrorInstance } from "./errors.js";
// Use bitwise representation of suggestion flags.
// See here: https://stackoverflow.com/questions/39359740/what-are-enum-flags-in-typescript/
//
// Given a number `flags` we can test `if (flags & Subcommands)` to see if we
// should be suggesting subcommands.
//
// This is more maintainable in the future if we add more options (e.g. if we
// distinguish between subcommand args and option args) as we can just add a
// number here instead of passing 3+ boolean flags everywhere.
export var SuggestionFlag;
(function (SuggestionFlag) {
    SuggestionFlag[SuggestionFlag["None"] = 0] = "None";
    SuggestionFlag[SuggestionFlag["Subcommands"] = 1] = "Subcommands";
    SuggestionFlag[SuggestionFlag["Options"] = 2] = "Options";
    SuggestionFlag[SuggestionFlag["Args"] = 4] = "Args";
    SuggestionFlag[SuggestionFlag["Any"] = 7] = "Any";
})(SuggestionFlag || (SuggestionFlag = {}));
export var SpecLocationSource;
(function (SpecLocationSource) {
    SpecLocationSource["GLOBAL"] = "global";
    SpecLocationSource["LOCAL"] = "local";
})(SpecLocationSource || (SpecLocationSource = {}));
export function makeArray(object) {
    return Array.isArray(object) ? object : [object];
}
export function firstMatchingToken(str, chars) {
    for (const char of str) {
        if (chars.has(char))
            return char;
    }
    return undefined;
}
export function makeArrayIfExists(obj) {
    return !obj ? null : makeArray(obj);
}
export function isOrHasValue(obj, valueToMatch) {
    return Array.isArray(obj) ? obj.includes(valueToMatch) : obj === valueToMatch;
}
export const TimeoutError = createErrorInstance("TimeoutError");
export async function withTimeout(time, promise) {
    return Promise.race([
        promise,
        new Promise((_, reject) => {
            setTimeout(() => {
                reject(new TimeoutError("Function timed out"));
            }, time);
        }),
    ]);
}
export const longestCommonPrefix = (strings) => {
    const sorted = strings.sort();
    const { 0: firstItem, [sorted.length - 1]: lastItem } = sorted;
    const firstItemLength = firstItem.length;
    let i = 0;
    while (i < firstItemLength && firstItem.charAt(i) === lastItem.charAt(i)) {
        i += 1;
    }
    return firstItem.slice(0, i);
};
export function findLast(values, predicate) {
    for (let i = values.length - 1; i >= 0; i -= 1) {
        if (predicate(values[i]))
            return values[i];
    }
    return undefined;
}
export function compareNamedObjectsAlphabetically(a, b) {
    const getName = (object) => typeof object === "string" ? object : makeArray(object.name)[0] || "";
    return getName(a).localeCompare(getName(b));
}
export const sleep = (ms) => new Promise((resolve) => {
    setTimeout(resolve, ms);
});
// Memoize a function (cache the most recent result based on the most recent args)
// Optionally can pass an equals function to determine whether or not the old arguments
// and new arguments are equal.
//
// e.g. let fn = (a, b) => a * 2
//
// If we memoize this then we recompute every time a or b changes. if we memoize with
// isEqual = ([a, b], [newA, newB]) => newA === a
// then we will only recompute when a changes.
export function memoizeOne(fn, isEqual) {
    let lastArgs = [];
    let lastResult;
    let hasBeenCalled = false;
    const areArgsEqual = isEqual || ((args, newArgs) => args.every((x, idx) => x === newArgs[idx]));
    return (...args) => {
        if (!hasBeenCalled || !areArgsEqual(lastArgs, args)) {
            hasBeenCalled = true;
            lastArgs = [...args];
            lastResult = fn(...args);
        }
        return lastResult;
    };
}
function isNonNullObj(v) {
    return typeof v === "object" && v !== null;
}
function isEmptyObject(v) {
    return isNonNullObj(v) && Object.keys(v).length === 0;
}
// TODO: to fix this we may want to have the default fields as Object.keys(A)
/**
 * If no fields are specified and A,B are not equal primitives/empty objects, this returns false
 * even if the objects are actually equal.
 */
export function fieldsAreEqual(A, B, fields) {
    if (A === B || (isEmptyObject(A) && isEmptyObject(B)))
        return true;
    if (!fields.length || !A || !B)
        return false;
    return fields.every((field) => {
        const aField = A[field];
        const bField = B[field];
        if (typeof aField !== typeof bField)
            return false;
        if (isNonNullObj(aField) && isNonNullObj(bField)) {
            if (Object.keys(aField).length !== Object.keys(bField).length) {
                return false;
            }
            return fieldsAreEqual(aField, bField, Object.keys(aField));
        }
        return aField === bField;
    });
}
export const splitPath = (path) => {
    const idx = path.lastIndexOf("/") + 1;
    return [path.slice(0, idx), path.slice(idx)];
};
export const ensureTrailingSlash = (str) => str.endsWith("/") ? str : `${str}/`;
// Outputs CWD with trailing `/`
export const getCWDForFilesAndFolders = (cwd, searchTerm) => {
    if (cwd === null)
        return "/";
    const [dirname] = splitPath(searchTerm);
    if (dirname === "") {
        return ensureTrailingSlash(cwd);
    }
    return dirname.startsWith("~/") || dirname.startsWith("/")
        ? dirname
        : `${cwd}/${dirname}`;
};
export function localProtocol(domain, path) {
    let modifiedDomain;
    if (domain === "path" && !window.fig?.constants?.newUriFormat) {
        modifiedDomain = "";
    }
    else {
        modifiedDomain = domain;
    }
    if (window.fig?.constants?.os === "windows") {
        return `https://fig.${modifiedDomain}/${path}`;
    }
    return `fig://${modifiedDomain}/${path}`;
}
export async function exponentialBackoff(options, fn) {
    let retries = 0;
    let delay = options.baseDelay;
    while (retries < options.maxRetries) {
        try {
            return await withTimeout(options.attemptTimeout, fn());
        }
        catch (_error) {
            retries += 1;
            delay *= 2;
            delay += Math.floor(Math.random() * options.jitter);
            await new Promise((resolve) => {
                setTimeout(resolve, delay);
            });
        }
    }
    throw new Error("Failed to execute function after all retries.");
}
//# sourceMappingURL=utils.js.map