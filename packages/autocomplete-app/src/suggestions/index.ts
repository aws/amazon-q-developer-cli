import { Result } from "@aws/amazon-q-developer-cli-fuzzysort";
import logger from "loglevel";
import {
  Option,
  Subcommand,
  Arg,
  Suggestion,
  SuggestionType,
} from "@aws/amazon-q-developer-cli-shared/internal";
import {
  makeArray,
  SuggestionFlags,
  SuggestionFlag,
  compareNamedObjectsAlphabetically,
  memoizeOne,
  localProtocol,
  fieldsAreEqual,
} from "@aws/amazon-q-developer-cli-shared/utils";
import {
  countEqualOptions,
  Annotation,
  TokenType,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import {
  getSetting,
  SETTINGS,
  SettingsMap,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { GeneratorState } from "../generators/helpers";

import {
  getFuzzyNameMatches,
  getPrefixNameMatches,
  getExactNameMatch,
  getQueryTermForSuggestion,
  bestScoreMatch,
} from "./helpers";

type StaticSuggestions = {
  subcommands: Suggestion[];
  additionalSuggestions: Suggestion[];
  options: Suggestion[];
};

const shouldAddSpaceToItem = (item: Subcommand | Option): boolean => {
  // 1. If there's a mandatory argument, insert the space regardless of whether
  //    any subcommands are required.
  const nextArg = makeArray(item.args || [])[0];
  const hasMandatoryArg = nextArg && !nextArg.isOptional;
  if (hasMandatoryArg) {
    return true;
  }
  // 2. If the spec explicitly says whether subcommands are required via
  //    `requiresSubcommand`, that takes precedence over heuristics.
  if (
    "requiresSubcommand" in item &&
    typeof item.requiresSubcommand === "boolean"
  ) {
    return item.requiresSubcommand;
  }
  // 3. Guess whether a space should be inserted. If there are no subcommands,
  //    don't insert a space, otherwise do.
  return (
    "subcommands" in item &&
    item.subcommands &&
    Object.keys(item.subcommands).length > 0
  );
};

const convertStringsToSuggestions = (
  isDangerous?: boolean,
  items?: (string | Fig.Suggestion)[],
  type?: SuggestionType,
): Suggestion[] => {
  try {
    return (items || []).map((item) =>
      typeof item === "string"
        ? { type, name: item, isDangerous }
        : { ...item, type: item.type || type },
    );
  } catch (err) {
    logger.error("Suggestions are incorrectly defined", err);
    return [];
  }
};

export const getStaticArgSuggestions = memoizeOne(
  (currentArg: Arg | null): Suggestion[] => {
    if (!currentArg) return [];
    const { suggestions, isDangerous } = currentArg;
    return convertStringsToSuggestions(isDangerous, suggestions, "arg");
  },
);

// Get all subcommand, options, and other suggestion
export const getStaticSuggestions = memoizeOne(
  (
    completionObj: Subcommand | null,
    passedOptions: Option[],
  ): StaticSuggestions => {
    if (!completionObj) {
      return { subcommands: [], additionalSuggestions: [], options: [] };
    }

    const subcommands = [
      ...new Set(Object.values(completionObj.subcommands)),
    ].map((subcommand) => ({
      ...subcommand,
      shouldAddSpace: shouldAddSpaceToItem(subcommand),
      type: "subcommand" as SuggestionType,
    }));

    const additionalSuggestions = convertStringsToSuggestions(
      completionObj.isDangerous,
      completionObj.additionalSuggestions,
    ).map((suggestion) =>
      suggestion.type || suggestion.icon
        ? suggestion
        : {
            ...suggestion,
            icon: localProtocol("template", "?color=628dad&badge=➡️"),
          },
    );

    const excludeOptions = new Set<string>();
    const unmetDepends = new Set<string>();

    passedOptions.forEach((option) => {
      option.exclusiveOn?.forEach((name) => excludeOptions.add(name));
      option.dependsOn?.forEach((name) => unmetDepends.add(name));
    });

    passedOptions.forEach((option) => {
      option.name.forEach((name) => unmetDepends.delete(name));
    });

    const optionValues = Object.values(completionObj.options).concat(
      Object.values(completionObj.persistentOptions),
    );
    const options = [...new Set(optionValues)]
      .filter((option) =>
        option.name.every((name) => !excludeOptions.has(name)),
      )
      .filter((option) => {
        let { isRepeatable } = option;
        if (!isRepeatable) {
          isRepeatable = 1;
        }
        return (
          isRepeatable === true ||
          countEqualOptions(option, passedOptions) < isRepeatable
        );
      })
      .map((option) => ({
        ...option,
        priority: option.name.some((name) => unmetDepends.has(name))
          ? 75
          : option.priority,
        shouldAddSpace: shouldAddSpaceToItem(option),
        separatorToAdd: (() => {
          // if the first argument isn't required, the separator should
          // never be inserted regardless of configuration. insertion should
          // only happen when the argument is required
          if (option.args[0]?.isOptional) {
            return undefined;
          }
          if (option.requiresSeparator) {
            const defaultSeparator = makeArray(
              completionObj.parserDirectives?.optionArgSeparators || "=",
            )[0];
            return typeof option.requiresSeparator !== "boolean"
              ? option.requiresSeparator
              : defaultSeparator;
          }
          if (option.requiresEquals) {
            return "=";
          }
          return undefined;
        })(),
        type: "option" as SuggestionType,
      }));

    return {
      subcommands: subcommands.sort(compareNamedObjectsAlphabetically),
      additionalSuggestions: additionalSuggestions.sort(
        compareNamedObjectsAlphabetically,
      ),
      options: options.sort(compareNamedObjectsAlphabetically),
    };
  },
);

export const getAllSuggestions = (
  currentArg: Arg | null,
  completionObj: Subcommand | null,
  passedOptions: Option[],
  suggestionFlags: SuggestionFlags,
  generatorStates: GeneratorState[],
  annotations: Annotation[],
): Suggestion[] => {
  // Memoized computation of static suggestions.
  const args = getStaticArgSuggestions(currentArg);
  const staticSuggestions = getStaticSuggestions(completionObj, passedOptions);

  const { subcommands, additionalSuggestions, options } = staticSuggestions;
  let suggestions = [];

  // Check if we should suggest subcommands.
  if (suggestionFlags & SuggestionFlag.Subcommands) {
    suggestions.push(...subcommands);
  }

  // Check if we should suggest args.
  if (suggestionFlags & SuggestionFlag.Args) {
    suggestions.push(...args);
    generatorStates.forEach((state) => {
      // If there's a filter term, we can keep old results until the new ones
      // load and just filter over them. But if there's a filter term, we might
      // not filter old results properly.
      if (!state.loading || !state.generator.getQueryTerm) {
        suggestions.push(...state.result);
      }
    });
  }

  suggestions.push(...additionalSuggestions);

  // Check if we should suggest options.
  if (suggestionFlags & SuggestionFlag.Options) {
    suggestions.push(...options);
  }

  const lastAnnotation = annotations[annotations.length - 1];
  const isInOptionChain =
    lastAnnotation.type === TokenType.Composite &&
    lastAnnotation.subtokens[lastAnnotation.subtokens.length - 1]?.type ===
      TokenType.Option;

  if (isInOptionChain) {
    const optionPrefix = lastAnnotation.text;
    const lastOptionName = `-${optionPrefix.charAt(optionPrefix.length - 1)}`;
    suggestions = suggestions.filter((suggestion) =>
      ["option", "arg"].includes(suggestion.type ?? ""),
    );

    const getShortName = (suggestion: Suggestion) =>
      makeArray(suggestion.name || "").find(
        (name) =>
          name.startsWith("-") && !name.startsWith("--") && name.length === 2,
      );

    // Make sure we still show the option itself if we have a mandatory arg
    // (only suggesting args) in an option chain.
    if (!(suggestionFlags & SuggestionFlag.Options)) {
      const exactMatch = options.find(
        (option) => getShortName(option) === lastOptionName,
      );
      if (exactMatch) {
        suggestions.push(exactMatch);
      }
    }

    // If the last option in the chain doesn't take a mandatory arg then we
    // should suggest
    suggestions = suggestions.map((suggestion) => {
      // All non-option suggestions should use an empty search term.
      if (suggestion.type === "arg") {
        return { ...suggestion, getQueryTerm: () => "" };
      }
      const shortName = getShortName(suggestion);
      if (!shortName) return suggestion;
      if (shortName === lastOptionName) {
        return {
          ...suggestion,
          name: makeArray(suggestion.name || []).map((name) =>
            name === shortName ? `${optionPrefix}` : name,
          ),
        };
      }
      const letter = shortName.charAt(1);
      return {
        ...suggestion,
        insertValue: `${optionPrefix}${letter}`,
        name: `${optionPrefix}${letter}`,
      };
    });
  }

  return suggestions;
};

export const isTemplateSuggestion = (
  suggestion: unknown,
): suggestion is Fig.TemplateSuggestion =>
  Object.prototype.hasOwnProperty.call(suggestion, "context");

const addAutoExecuteSuggestion = (
  suggestions: Suggestion[],
  searchTerm: string,
  suggestCurrentToken: boolean,
): Suggestion[] => {
  const autoExecuteSuggestion = (
    name: string,
    description: string,
    originalType?: SuggestionType,
  ): Suggestion => ({
    name,
    type: "auto-execute" as SuggestionType,
    originalType,
    insertValue: "\n",
    description,
  });
  // Special case handling for when the search term points to a directory path.
  if (
    searchTerm === "." ||
    (searchTerm.endsWith("/") &&
      suggestions.find(
        (x) =>
          // TODO: use a better way to determine if x.generators. is a folders generator
          isTemplateSuggestion(x) && x.context.templateType === "folders",
      ))
  ) {
    return [
      autoExecuteSuggestion(
        searchTerm === "." ? searchTerm : "↪",
        "Enter the current directory",
        "folder",
      ),
      ...suggestions,
    ];
  }
  if (suggestCurrentToken) {
    return [
      autoExecuteSuggestion(searchTerm, "Enter the current argument"),
      ...suggestions,
    ];
  }
  return suggestions;
};

export const deduplicateSuggestions = (
  suggestions: Suggestion[],
  fields: (keyof Suggestion)[],
): Suggestion[] => {
  const deduplicated: Suggestion[] = [];
  for (const suggestion of suggestions) {
    const isDuplicated =
      deduplicated.filter((dd) => fieldsAreEqual(suggestion, dd, fields))
        .length > 0;
    if (!isDuplicated) {
      deduplicated.push(suggestion);
    }
  }
  return deduplicated;
};

const getPartialMatch = (
  searchTerms: string[],
  queryTerm: string,
  fuzzySearchEnabled: boolean,
) => {
  const prefixMatches = getPrefixNameMatches(searchTerms, queryTerm);
  const prefixMatch = bestScoreMatch(prefixMatches);

  let fuzzyMatches: Array<Result | null> | undefined;
  let fuzzyMatch: Result | null | undefined;

  if (fuzzySearchEnabled) {
    fuzzyMatches = getFuzzyNameMatches(searchTerms, queryTerm);
    fuzzyMatch = bestScoreMatch(fuzzyMatches);
  }

  const match = fuzzySearchEnabled ? fuzzyMatch : prefixMatch;

  if (match) {
    let { score } = match;
    if (fuzzySearchEnabled && prefixMatch) {
      // suggestions matching the prefix should always be at the top ordered by frecency so
      // since fuzzy search always returns integer negative scores (with 0 as an exact match),
      // we assign to suggestions matching both prefix and fuzzy a score: -1 < s < 0
      // In this way we ensure they are always at the top (as it would be with fuzzy match),
      // but they are also ordered using the same prefix criteria
      score = prefixMatch.score / 10;
    }
    return {
      fuzzyMatchData: fuzzyMatches,
      score,
    };
  }
  return undefined;
};

export const filterSuggestions = (
  suggestions: Suggestion[],
  searchTerm: string,
  fuzzySearchEnabled: boolean,
  suggestCurrentToken: boolean,
  settings: SettingsMap | undefined = undefined,
): Suggestion[] => {
  const shouldHideAutoExecute =
    getSetting<boolean>(SETTINGS.HIDE_AUTO_EXECUTE_SUGGESTION, false) ||
    getSetting<boolean>(SETTINGS.ONLY_SHOW_ON_TAB, false);
  const execDangerous = settings?.[
    SETTINGS.IMMEDIATELY_RUN_DANGEROUS_COMMANDS
  ] as boolean;

  if (!searchTerm) {
    const filteredSuggestions = suggestions.filter(
      (suggestion) => !suggestion.hidden,
    );
    const execAfterSpace = settings?.[
      SETTINGS.IMMEDIATELY_EXEC_AFTER_SPACE
    ] as boolean;
    if (
      execAfterSpace &&
      filteredSuggestions.length > 0 &&
      !shouldHideAutoExecute
    ) {
      filteredSuggestions.unshift({
        name: "↪",
        type: "auto-execute" as SuggestionType,
        insertValue: "\n",
        description: "Immediately execute",
      });
    }
    return filteredSuggestions;
  }

  const caseSensitiveMatchArr: Suggestion[] = [];
  const nonCaseSensitiveMatchArr: Suggestion[] = [];
  const nonExactMatches: Array<{ suggestion: Suggestion; score: number }> = [];
  let autoExecuteAdded = false;

  // try to match either insertValue or any name
  for (const suggestion of suggestions) {
    const { name, type, insertValue, displayName } = suggestion;
    const queryTerm = getQueryTermForSuggestion(suggestion, searchTerm);
    const exactMatchQueryTerm = type === "folder" ? `${queryTerm}/` : queryTerm;
    const exactNameMatch = getExactNameMatch(suggestion, exactMatchQueryTerm);
    const names = makeArray(name ?? []);

    const pushNonExactMatch = (searchTerms: string[]) => {
      const displayNameMatch = getPartialMatch(
        searchTerms,
        queryTerm,
        fuzzySearchEnabled,
      );
      if (displayNameMatch) {
        const { fuzzyMatchData, score } = displayNameMatch;
        nonExactMatches.push({
          suggestion: { ...suggestion, fuzzyMatchData },
          score,
        });
      }
    };

    if (exactNameMatch) {
      const suggestionsToAdd: Suggestion[] = [suggestion];

      const arg = suggestion.args?.[0];
      const shouldAddAutoExecute =
        (!names[0] || !insertValue || names[0] === insertValue) &&
        (!suggestion.isDangerous || execDangerous) &&
        (!arg || arg.isOptional);

      if (shouldAddAutoExecute && !shouldHideAutoExecute && !autoExecuteAdded) {
        // replace the current suggestion with an autoexecute suggestion and add it
        suggestionsToAdd.unshift({
          ...suggestion,
          type: "auto-execute" as SuggestionType,
          icon: localProtocol("icon", "?type=carrot"),
          name: type === "folder" ? exactNameMatch.slice(0, -1) : name,
          description: type === "folder" ? "folder" : suggestion.description,
          insertValue: "\n",
        });
        autoExecuteAdded = true;
      }
      if (queryTerm === exactMatchQueryTerm) {
        caseSensitiveMatchArr.push(...suggestionsToAdd);
      } else {
        nonCaseSensitiveMatchArr.push(...suggestionsToAdd);
      }
    } else if (!suggestion.hidden) {
      pushNonExactMatch(makeArray(name ?? []));
    }

    if (displayName) {
      pushNonExactMatch([displayName]);
    }
  }

  nonExactMatches.sort((a, b) => {
    if (a.score !== b.score) {
      return b.score - a.score;
    }
    return (b.suggestion.priority || 0) - (a.suggestion.priority || 0);
  });

  let filteredSuggestions = [
    ...caseSensitiveMatchArr,
    ...nonCaseSensitiveMatchArr,
    ...nonExactMatches.map(({ suggestion }) => suggestion),
  ];

  if (
    filteredSuggestions.length > 0 &&
    !autoExecuteAdded &&
    !shouldHideAutoExecute
  ) {
    // Eventually add suggest current token suggestion
    filteredSuggestions = addAutoExecuteSuggestion(
      filteredSuggestions,
      searchTerm,
      suggestCurrentToken,
    );
  }

  return filteredSuggestions;
};
