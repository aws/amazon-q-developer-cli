import {
  Annotation,
  TokenType,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { Subcommand } from "@aws/amazon-q-developer-cli-shared/internal";
import { describe, expect, it } from "vitest";
import { getTemplateSuggestions } from "../templateSuggestionsGenerator";
import { GeneratorContext } from "../helpers";

const defaultContext: GeneratorContext = {
  annotations: [] as Annotation[],
  tokenArray: [] as string[],
  currentWorkingDirectory: "/",
  currentProcess: "zsh",
  sshPrefix: "",
  searchTerm: "",
  environmentVariables: {},
};

describe.todo("getTemplateSuggestions", () => {
  describe("help template", () => {
    const spec: Subcommand = {
      name: ["foo"],
      subcommands: {
        bar: {
          name: ["bar"],
          subcommands: {
            zoo: {
              name: ["zoo"],
              subcommands: {},
              options: {},
              parserDirectives: undefined,
              persistentOptions: {},
              args: [],
            },
          },
          options: {
            "--help": {
              name: ["-h", "--help"],
              args: [
                {
                  generators: [{ template: "help" }],
                },
              ],
            },
            "-h": {
              name: ["-h", "--help"],
              args: [
                {
                  generators: [{ template: "help" }],
                },
              ],
            },
          },
          parserDirectives: undefined,
          persistentOptions: {},
          args: [],
        },
        cat: {
          name: ["cat"],
          subcommands: {
            help: {
              name: ["help"],
              subcommands: {},
              options: {},
              parserDirectives: undefined,
              persistentOptions: {},
              args: [{ generators: [{ template: "help" }] }],
            },
          },
          options: {},
          parserDirectives: undefined,
          persistentOptions: {},
          args: [],
        },
        zar: {
          name: ["zar", "zarzar"],
          subcommands: {},
          options: {},
          parserDirectives: undefined,
          persistentOptions: {},
          args: [],
        },
        zarzar: {
          name: ["zar", "zarzar"],
          subcommands: {},
          options: {},
          parserDirectives: undefined,
          persistentOptions: {},
          args: [],
        },
        help: {
          name: ["help", "h"],
          subcommands: {},
          options: {},
          parserDirectives: undefined,
          persistentOptions: {},
          args: [{ generators: [{ template: "help" }] }],
        },
        h: {
          name: ["help", "h"],
          subcommands: {},
          options: {},
          parserDirectives: undefined,
          persistentOptions: {},
          args: [{ generators: [{ template: "help" }] }],
        },
      },
      options: {},
      parserDirectives: undefined,
      persistentOptions: {},
      args: [],
    };

    it("should return all the siblings of the subcommand where the 'help' template is added", async () => {
      const context: GeneratorContext = {
        ...defaultContext,
        annotations: [
          {
            text: "foo",
            type: TokenType.Subcommand,
            spec,
          },
          {
            text: "help",
            type: TokenType.Subcommand,
          },
          {
            text: "",
            type: TokenType.SubcommandArg,
          },
        ],
        tokenArray: ["foo", "help", ""],
      };
      expect(
        (await getTemplateSuggestions({ template: "help" }, context)).map(
          (v) => v.name,
        ),
      ).toEqual(["bar", "cat", "zar", "zarzar"]);
    });

    it("should return an empty array if there are no siblings to the subcommand containing `help`", async () => {
      const context: GeneratorContext = {
        ...defaultContext,
        annotations: [
          {
            text: "foo",
            type: TokenType.Subcommand,
            spec,
          },
          {
            text: "cat",
            type: TokenType.Subcommand,
            spec,
          },
          {
            text: "help",
            type: TokenType.Subcommand,
          },
        ],
        tokenArray: ["foo", "cat", "help"],
      };
      expect(
        (await getTemplateSuggestions({ template: "help" }, context)).map(
          (v) => v.name,
        ),
      ).toEqual([]);
    });

    it("should return all the subcommands of the subcommand parent of the option that has the `help` template", async () => {
      const context: GeneratorContext = {
        ...defaultContext,
        annotations: [
          {
            text: "foo",
            type: TokenType.Subcommand,
            spec,
          },
          {
            text: "bar",
            type: TokenType.Subcommand,
          },
          {
            text: "--help",
            type: TokenType.Option,
          },
          {
            text: "",
            type: TokenType.OptionArg,
          },
        ],
        tokenArray: ["foo", "bar", "--help", ""],
      };
      expect(
        (await getTemplateSuggestions({ template: "help" }, context)).map(
          (v) => v.name,
        ),
      ).toEqual(["zoo"]);
    });
  });
});
