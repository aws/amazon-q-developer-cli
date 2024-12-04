import { localProtocol } from "@aws/amazon-q-developer-cli-shared/utils";

const generateSpec = async (
  tokenArray: string[] | undefined,
): Promise<Fig.Subcommand> => {
  // Throw an error if we find an option in the tokenArray (allows us to
  // test error handling.
  if (tokenArray && tokenArray.find((x) => x.startsWith("-"))) {
    throw new Error("Error in generateSpec");
  }
  return {
    name: "generateSpec",
    description: "Fake generated spec",
    subcommands: ["these", "are", "generated", "subcommands"].map((name) => ({
      name,
      icon: localProtocol("icon", "?type=commandkey"),
      args: {
        name: "string",
        suggestions: ["generated", "arg", "suggestions"],
      },
    })),
    args: {},
    options: [{ name: "--genOpt" }],
  };
};

const partialCmd: Fig.Subcommand = {
  name: "",
  description:
    "Partial test subcommand with fixed set of options for use in unit tests",
  // Covers (generateSpec/loadSpec/takesSpecialArg/normal) subcommand next tokens.
  subcommands: [
    {
      name: "generateSpec",
      description: "subcommand that generates a new spec",
      generateSpec,
      options: [{ name: "--anOpt" }],
    },
    {
      name: "loadSpec",
      description: "subcommand that loads a new spec",
      loadSpec: "loadSpecName",
    },
    {
      name: "module",
      description: "subcommand that takes isModule arg",
      args: { isModule: "python/" },
    },
    {
      name: "normal",
      args: { isOptional: true },
    },
  ],
  // Covers (mandatory/variadic/optional/null/requiresEquals/requiresSeparator) optionArgStates
  options: [
    {
      name: ["-m", "--man"],
      description: "option with mandatory arg",
      args: {},
    },
    {
      name: ["-o", "--opt"],
      description: "option with optional arg",
      args: { isOptional: true },
    },
    {
      name: "-v",
      description: "option with variadic arg",
      args: { isVariadic: true },
    },
    {
      name: "--multiple",
      description: "option with multiple mandatory args",
      args: [{}, {}],
    },
    {
      name: ["-n", "--none"],
      description: "option with no args",
    },
    {
      name: "-pnfm",
      description: "posix non-compliant option with mandatory arg",
      args: {},
    },
    {
      name: "--req-eq",
      requiresEquals: true,
      description: "option with requiresEquals: true",
      args: {},
    },
    {
      name: "--req-sep",
      requiresSeparator: ":",
      description: "option with requiresSeparator set",
      args: {},
    },
    {
      name: "-pnfo",
      description: "posix non-compliant option with optional arg",
      args: { isOptional: true },
    },
    {
      name: "-pnfn",
      description: "posix non-compliant option with no arg",
    },
  ],
};

const isCommandCmd: Fig.Subcommand = {
  name: "",
  subcommands: [
    {
      name: "sudo",
      args: { isCommand: true },
    },
  ],
};

const recursiveLoadSpecCmd: Fig.Subcommand = {
  name: "",
  subcommands: [
    {
      name: "recursiveLoadSpec",
      description: "subcommand that loads a new spec",
      loadSpec: "recursiveLoadSpecNested",
    },
    {
      name: "recursiveLoadSpecNested",
      subcommands: [
        { name: "nestedSubcommand", loadSpec: "recursiveLoadSpecNestedNested" },
      ],
    },
    {
      name: "recursiveLoadSpecNestedNested",
      subcommands: [
        { name: "doubleNestedSubcommand", args: [{ name: "arg1" }] },
      ],
    },
  ],
};

// Export subcommands to easily expose to tests.
export const mandatoryCmd = {
  ...partialCmd,
  name: "man",
  description: "Subcommand that takes multiple mandatory args",
  args: [{ name: "arg1" }, { name: "arg2" }],
};

export const optionArgSeparatorsCmd: Fig.Subcommand = {
  ...partialCmd,
  name: "oas",
  parserDirectives: {
    optionArgSeparators: ["=", ":"],
  },
  args: [{ name: "arg1" }, { name: "arg2" }],
};

export const optionalCmd = {
  ...partialCmd,
  name: "opt",
  description: "Subcommand that takes multiple optional args",
  args: [
    { name: "arg1", isOptional: true },
    { name: "arg2", isOptional: true },
  ],
};

export const variadicCmd = {
  ...partialCmd,
  name: "var",
  description: "Subcommand that takes variadic arg",
  args: [{ name: "argv", variadic: true }],
};

export const noArgCmd = {
  ...partialCmd,
  name: "none",
  description: "Subcommand that takes no args",
};

export const pnfCmd = {
  ...partialCmd,
  name: "pnf",
  description: "Subcommand with posix noncompliant flags",
  parserDirectives: {
    flagsArePosixNoncompliant: true,
  },
};

// optionsMustPrecedeArguments spec
export const ompaCmd = {
  ...partialCmd,
  name: "ompa",
  description: "Subcommand with options that must precede arguments",
  parserDirectives: {
    optionsMustPrecedeArguments: true,
  },
  args: [{ name: "argv" }],
};

export const cmdSpec = {
  ...partialCmd,
  name: "cmd",
  description: "Test completion spec used for unit tests",
  subcommands: [
    mandatoryCmd,
    optionArgSeparatorsCmd,
    optionalCmd,
    variadicCmd,
    noArgCmd,
    pnfCmd,
    ompaCmd,
    ...(partialCmd.subcommands || []),
    ...(recursiveLoadSpecCmd.subcommands || []),
    ...(isCommandCmd.subcommands || []),
  ],
};

export default cmdSpec;
