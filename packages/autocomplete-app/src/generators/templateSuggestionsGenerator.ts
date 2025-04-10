import logger from "loglevel";
import {
  Annotation,
  TokenType,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { makeArray, findLast } from "@aws/amazon-q-developer-cli-shared/utils";
import { GeneratorContext } from "./helpers";
import { getHistoryArgSuggestions } from "../history";

const getHelpSuggestions = (annotations: Annotation[]) => {
  const helpSuggestions: Fig.TemplateSuggestion[] = [];
  const rootAnnotation = annotations[0];
  if (rootAnnotation && "spec" in rootAnnotation) {
    // the annotation containing the generator
    const triggeringAnnotation = findLast(
      annotations,
      (a) => a.type === TokenType.Subcommand || a.type === TokenType.Option,
    );
    if (!triggeringAnnotation) return [];
    let lastSubcommandSpec = rootAnnotation.spec;
    let triggeringAliases = new Set<string>();
    for (let i = 1; i < annotations.length; i += 1) {
      if (annotations[i].type === TokenType.Subcommand) {
        if (annotations[i] === triggeringAnnotation) {
          // add triggering aliases
          triggeringAliases = new Set(
            lastSubcommandSpec.subcommands[annotations[i].text].name,
          );
        } else {
          // we resolve to the spec for the last subcommand available that satisfies the condition
          // of not being the same subcommand that triggered the generator
          lastSubcommandSpec =
            lastSubcommandSpec.subcommands[annotations[i].text];
        }
      }
    }
    helpSuggestions.push(
      ...Object.keys(lastSubcommandSpec.subcommands)
        .filter((v) => !triggeringAliases.has(v))
        .map(
          (v) =>
            ({
              type: "special",
              name: v,
              insertValue: v,
              isDangerous: false,
              context: { templateType: "help" },
            }) as Fig.TemplateSuggestion,
        ),
    );
  }
  return helpSuggestions;
};

export async function getTemplateSuggestions(
  generator: Fig.Generator,
  context: GeneratorContext,
): Promise<Fig.Suggestion[]> {
  const { template, filterTemplateSuggestions } = generator;
  const { currentProcess, annotations } = context;
  if (!template) {
    return [];
  }

  const suggestions: Fig.TemplateSuggestion[] = [];
  const typesToInclude = new Set(makeArray(template));

  if (typesToInclude.has("history")) {
    try {
      const historySuggestions = getHistoryArgSuggestions(
        annotations,
        currentProcess,
      );
      suggestions.push(...historySuggestions);
    } catch (_err) {
      logger.error("template suggestion did not work for template: history");
    }
  }

  if (typesToInclude.has("help")) {
    suggestions.push(...getHelpSuggestions(annotations));
  }

  return filterTemplateSuggestions
    ? filterTemplateSuggestions(suggestions)
    : suggestions;
}
