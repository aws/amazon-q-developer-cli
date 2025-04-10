import {
  Metadata,
  convertSubcommand,
  initializeDefault,
} from "@fig/autocomplete-shared";
import logger from "loglevel";
import { folders, filepaths } from "@fig/autocomplete-generators";
import * as Internal from "@aws/amazon-q-developer-cli-shared/internal";
import {
  SuggestionFlag,
  SuggestionFlags,
  makeArray,
  SpecLocationSource,
} from "@aws/amazon-q-developer-cli-shared/utils";
import { Command, getCommand } from "@aws/amazon-q-developer-cli-shell-parser";
import {
  findOption,
  getCurrentArg,
  parseArguments as defaultParseArguments,
  updateArgState,
  findSubcommand,
  createArgState,
} from "../src/parseArguments";
import { loadSubcommandCached } from "../src/loadSpec";
import { cmdSpec } from "./mocks/spec";
import { resetCaches } from "../src/caches";
import {
  afterEach,
  beforeEach,
  beforeAll,
  describe,
  it,
  expect,
  Mock,
  vi,
} from "vitest";

vi.mock("../src/loadSpec", () => ({
  ...vi.importActual("../src/loadSpec.ts"),
  loadSubcommandCached: vi.fn(),
}));

const cmd = convertSubcommand(cmdSpec, initializeDefault);

const parseArguments = (
  command: Command | null,
  context: Fig.ShellContext,
  isParsingHistory?: boolean,
  localLogger?: logger.Logger,
) => defaultParseArguments(command, context, isParsingHistory, localLogger);

beforeAll(() => {
  global.fig = {
    ...global.fig,
    settings: {},
  };
});

beforeEach(() => {
  resetCaches();
});

const emptyArgState = { args: null, index: 0 };
const noArgState = { args: [], index: 0 };

describe("createArgState", () => {
  it("should not convert a generator if no filepaths/folders template is provided", () => {
    const generators: Fig.Generator[] = [{ script: ["foo"] }];
    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual(generators);
  });
  it("should convert a generator when it contains a filepaths/folders template", () => {
    const generators: Fig.Generator[] = [
      { script: ["foo"] },
      { template: "folders" },
      { custom: async () => [], template: "filepaths" },
    ];
    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual([
      { script: ["foo"] },
      folders,
      filepaths,
    ]);
  });

  it("should preserve templates other than filepaths and folders", () => {
    const generators: Fig.Generator[] = [
      { script: ["foo"] },
      { template: ["folders", "help"] },
      { custom: async () => [], template: "filepaths" },
    ];
    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual([
      { script: ["foo"] },
      folders,
      filepaths,
    ]);
  });

  it("should prefer filepaths generator over folders", () => {
    const generators: Fig.Generator[] = [
      { template: ["filepaths", "folders"] },
    ];
    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual([filepaths]);
  });

  it("should not add a filepaths/folders template if one is already present", () => {
    const generators: Fig.Generator[] = [{ template: "folders" }, folders];
    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual([folders]);
  });

  it("should preserve filterTemplateSuggestions", () => {
    const filterTemplateSuggestions = () => [];
    const generators: Fig.Generator[] = [
      {
        template: "filepaths",
        filterTemplateSuggestions,
      },
      { script: ["foo"] },
    ];

    const baseArgs: Internal.Arg[] = [{ generators }];
    const state = createArgState(baseArgs);
    expect(state.args![0].generators).toEqual([
      Object.assign(filepaths, { filterTemplateSuggestions }),
      { script: ["foo"] },
    ]);
  });
});
describe("updateArgState", () => {
  it("works with empty", () => {
    expect(updateArgState(emptyArgState)).toEqual(emptyArgState);
    expect(updateArgState(noArgState)).toEqual(emptyArgState);
  });

  it("works with variadic", () => {
    const withVariadic = {
      args: [{ generators: [] }, { isVariadic: true, generators: [] }],
      index: 0,
    };
    const variadicStep = { ...withVariadic, index: 1 };

    expect(updateArgState(withVariadic)).toEqual(variadicStep);
    expect(updateArgState(variadicStep)).toEqual({
      ...variadicStep,
      variadicCount: 1,
    });
  });

  it("works with multiple args", () => {
    const multipleArgs = {
      args: [{ generators: [] }, { generators: [] }, { generators: [] }],
      index: 1,
    };
    const multipleArgsNext = { ...multipleArgs, index: 2 };

    expect(updateArgState(multipleArgs)).toEqual(multipleArgsNext);
    expect(updateArgState(multipleArgsNext)).toEqual(emptyArgState);
  });
});

describe("getCurrentArg", () => {
  it("works with empty", () => {
    expect(getCurrentArg(emptyArgState)).toEqual(null);
    expect(getCurrentArg(noArgState)).toBe(null);
  });

  it("works with arg", () => {
    const args = [{ generators: [] }, { generators: [] }];
    expect(getCurrentArg({ args, index: 1 })).toBe(args[1]);
    expect(getCurrentArg({ args, index: 2 })).toBe(null);
  });
});

const baseSpec: Internal.Subcommand = {
  name: ["test"],
  subcommands: {},
  args: [],
  parserDirectives: {},
  options: {},
  persistentOptions: {},
};

describe("findOption", () => {
  const firstOption: Internal.Option = { name: ["-m", "--msg"], args: [] };
  const secondOption: Internal.Option = { name: ["-o"], args: [] };
  const specWithOptions: Internal.Subcommand = {
    ...baseSpec,
    options: {
      "-m": firstOption,
      "--msg": firstOption,
      "-o": secondOption,
    },
  };

  it("works with options", () => {
    expect(findOption(specWithOptions, "-m")).toBe(firstOption);
    expect(findOption(specWithOptions, "--msg")).toBe(firstOption);
    expect(findOption(specWithOptions, "-o")).toBe(secondOption);
  });

  it("throws with invalid options", () => {
    expect(() => findOption(specWithOptions, "--notAnOption")).toThrow();
  });
});

describe("findSubcommand", () => {
  const firstSubcommand = { ...baseSpec, name: ["name1", "name2"] };
  const secondSubcommand = { ...baseSpec, name: ["name"] };
  const specWithSubcommands: Internal.Subcommand = {
    ...baseSpec,
    subcommands: {
      name1: firstSubcommand,
      name2: firstSubcommand,
      name: secondSubcommand,
    },
  };

  it("works with options", () => {
    expect(findSubcommand(specWithSubcommands, "name1")).toBe(firstSubcommand);
    expect(findSubcommand(specWithSubcommands, "name2")).toBe(firstSubcommand);
    expect(findSubcommand(specWithSubcommands, "name")).toBe(secondSubcommand);
  });

  it("throws with invalid options", () => {
    expect(() =>
      findSubcommand(specWithSubcommands, "notASubcommand"),
    ).toThrow();
  });
});

const emptyContext: Fig.ShellContext = {
  currentWorkingDirectory: "",
  currentProcess: "",
  sshPrefix: "",
  environmentVariables: {},
};

// Note we don't try to cover certain caught errors (e.g. when
// updateStateForSubcommand throws) because they are implementation details and
// we are concerned only with behavior.
// See https://teamgaslight.com/blog/testing-behavior-vs-testing-implementation
describe.todo("parseArguments", () => {
  // Function that runs a basic test for a command that doesn't load/generate new spec.
  const basicTestWithSpec =
    (completionObj: Internal.Subcommand) =>
    async (
      buffer: string,
      flags: SuggestionFlags,
      currentArg: Metadata.ArgMeta | null,
      searchTerm: string,
    ) => {
      // Mock loading of the provided spec.
      (loadSubcommandCached as Mock).mockResolvedValueOnce(cmd);

      const command = getCommand(buffer, {});
      const result = await parseArguments(command, emptyContext);
      expect({
        currentArg: result.currentArg,
        flags: result.suggestionFlags,
        searchTerm: result.searchTerm,
        completionObj: result.completionObj,
      }).toEqual({
        currentArg: makeArray(currentArg)[0],
        flags,
        searchTerm,
        completionObj,
      });

      // return result;
    };

  const { pnf, man, var: vrd, opt, ompa, oas } = cmd.subcommands;

  const testCmd = basicTestWithSpec(cmd);
  const testPnf = basicTestWithSpec(pnf);
  const testMan = basicTestWithSpec(man);
  const testOas = basicTestWithSpec(oas);
  const testVar = basicTestWithSpec(vrd);
  const testOpt = basicTestWithSpec(opt);
  const testOmpa = basicTestWithSpec(ompa);

  const optionArgs = (subcommand: Internal.Subcommand, option: string) =>
    subcommand.options[option].args;
  const notSubcommands = SuggestionFlag.Any & ~SuggestionFlag.Subcommands;

  it.each([
    ["cmd man ", SuggestionFlag.Any, man.args[0], ""],
    ["cmd man arg1 arg2 ", notSubcommands, null, ""],
  ])("works with basic subcommand: %p", testMan);

  it.each([
    ["cmd -o ", SuggestionFlag.Any, optionArgs(cmd, "-o")[0], ""],
    ["cmd --opt ", SuggestionFlag.Any, optionArgs(cmd, "--opt")[0], ""],
  ])("works with basic options: %p", testCmd);

  it.each([
    // Test pnf options.
    ["cmd pnf -pnfo ", SuggestionFlag.Any, optionArgs(pnf, "-pnfo")[0], ""],
  ])("works with basic options: %p", testPnf);

  // End of options
  it.each([
    ["cmd man --", SuggestionFlag.Any, man.args[0], "--"],
    ["cmd man -- ", SuggestionFlag.Args, man.args[0], ""],
    ["cmd man -- -o ", SuggestionFlag.Args, man.args[1], ""],
  ])("works with end of options: %p", testMan);

  // Note that the arg object and suggestion flags are the same here.
  it.each([
    ["cmd man ", SuggestionFlag.Any, man.args[0], ""],
    ["cmd man normal", SuggestionFlag.Any, man.args[0], "normal"],
    ["cmd man a", SuggestionFlag.Any, man.args[0], "a"],
    ["cmd man -o", SuggestionFlag.Any, optionArgs(man, "-o")[0], "-o"],
  ])("invariant under most final tokens: %p", testMan);

  it.each([
    // User has typed in = arg successfully for optional or mandatory option arg.
    ["cmd man --opt=arg ", SuggestionFlag.Any, man.args[0], ""],
    ["cmd man --man=arg ", SuggestionFlag.Any, man.args[0], ""],
    // User tried to pass arg to option that doesn't take any, consume as arg instead.
    ["cmd man --none=arg ", notSubcommands, man.args[1], ""],
    // User is still typing in arg for option.
    [
      "cmd man --opt=arg",
      SuggestionFlag.Args,
      optionArgs(man, "--opt")[0],
      "arg",
    ],
    [
      "cmd man --man=arg",
      SuggestionFlag.Args,
      optionArgs(man, "--man")[0],
      "arg",
    ],
    // User is still typing in arg for option that doesn't take arg.
    ["cmd man --none=arg", SuggestionFlag.Any, man.args[0], "--none=arg"],
  ])("option with equals sign: %p", testMan);

  it.each([
    // Test pnf options.
    ["cmd pnf -pnfo=arg ", SuggestionFlag.Any, null, ""],
    ["cmd pnf -pnfm=arg ", SuggestionFlag.Any, null, ""],
    [
      "cmd pnf -pnfo=arg",
      SuggestionFlag.Args,
      optionArgs(pnf, "-pnfo")[0],
      "arg",
    ],
    [
      "cmd pnf -pnfm=arg",
      SuggestionFlag.Args,
      optionArgs(pnf, "-pnfm")[0],
      "arg",
    ],
  ])("pnf options with equals sign: %p", testPnf);

  it.each([
    // Test req-eq options.
    ["cmd man --req-eq=arg ", SuggestionFlag.Any, man.args[0], ""],
    ["cmd man --req-sep=arg ", SuggestionFlag.Any, man.args[0], ""],
    [
      "cmd man --req-eq=",
      SuggestionFlag.Args,
      optionArgs(man, "--req-eq")[0],
      "",
    ],
    [
      "cmd man --req-sep=",
      SuggestionFlag.Args,
      optionArgs(man, "--req-sep")[0],
      "",
    ],
    ["cmd man --req-eq ", SuggestionFlag.Any, man.args[0], ""],
  ])("options that require an equal: %p", testMan);

  it.each([
    // Test req-sep options.
    ["cmd oas --req-sep:arg ", SuggestionFlag.Any, oas.args[0], ""],
    ["cmd oas --req-sep=arg ", SuggestionFlag.Any, oas.args[0], ""],
    [
      "cmd oas --req-sep:",
      SuggestionFlag.Args,
      optionArgs(oas, "--req-sep")[0],
      "",
    ],
    [
      "cmd oas --req-sep=",
      SuggestionFlag.Args,
      optionArgs(oas, "--req-sep")[0],
      "",
    ],
    ["cmd oas --req-sep ", SuggestionFlag.Any, oas.args[0], ""],
  ])("options that require a separator different from equals: %p", testOas);

  // Other exceptions to invariance under final token (chained options)
  it.each([
    // Passing arg as chaining, optional.
    ["cmd man -oarg", SuggestionFlag.Args, optionArgs(man, "-o")[0], "arg"],
    ["cmd man -o=arg", SuggestionFlag.Args, optionArgs(man, "-o")[0], "=arg"],
    ["cmd man -oarg ", SuggestionFlag.Any, man.args[0], ""],
    // Passing arg as chaining, mandatory.
    ["cmd man -marg", SuggestionFlag.Args, optionArgs(man, "-m")[0], "arg"],
    ["cmd man -m=arg", SuggestionFlag.Args, optionArgs(man, "-m")[0], "=arg"],
    ["cmd man -marg ", SuggestionFlag.Any, man.args[0], ""],
    // Chained options with chained arg.
    ["cmd man -omarg", SuggestionFlag.Args, optionArgs(man, "-m")[0], "arg"],
    ["cmd man -moarg", SuggestionFlag.Args, optionArgs(man, "-m")[0], "oarg"],
    // Invalid chained arg with option -n that doesn't take an arg.
    ["cmd man -nb", SuggestionFlag.Any, man.args[0], "-nb"],
    ["cmd man -on", SuggestionFlag.Any, null, "-on"],
    ["cmd man --opt", SuggestionFlag.Any, man.args[0], "--opt"],
    ["cmd man --opt=", SuggestionFlag.Args, optionArgs(man, "-o")[0], ""],
  ])("chained options: %p", testMan);

  it.each([
    ["cmd -m arg ", SuggestionFlag.Any, null, ""],
    ["cmd -o arg ", SuggestionFlag.Any, null, ""],
    ["cmd --man arg ", SuggestionFlag.Any, null, ""],
    ["cmd --opt arg ", SuggestionFlag.Any, null, ""],
    [
      "cmd -v arg ",
      SuggestionFlag.Args | SuggestionFlag.Options,
      optionArgs(cmd, "-v")[0],
      "",
    ],
    ["cmd -n ", SuggestionFlag.Any, null, ""],
  ])("passing args to options: %p", testCmd);

  it.each([
    ["cmd ompa argument ", SuggestionFlag.Args, null, ""],
    ["cmd ompa -o ", SuggestionFlag.Any, ompa.args[0], ""],
    ["cmd ompa -m ", SuggestionFlag.Args, optionArgs(ompa, "-m")[0], ""],
    ["cmd ompa -m argument ", SuggestionFlag.Any, ompa.args[0], ""],
  ])("passing options after args: %p", testOmpa);

  // Test matrix in different scenarios (before arg subcommands, within arg
  // subcommands, finished arg subcommands) x (endOfOptions/not endOfOptions)
  describe("arg matrix", () => {
    it.each([
      ["cmd man -m ", SuggestionFlag.Args, optionArgs(man, "-m")[0], ""],
      ["cmd man -o ", SuggestionFlag.Any, man.args[0], ""],
      ["cmd man -v ", SuggestionFlag.Args, optionArgs(man, "-v")[0], ""],
      ["cmd man -n ", SuggestionFlag.Any, man.args[0], ""],
      ["cmd man -marg", SuggestionFlag.Args, optionArgs(man, "-m")[0], "arg"],
      ["cmd man -oarg", SuggestionFlag.Args, optionArgs(man, "-o")[0], "arg"],
    ])("mandatory subcommand: %p", testMan);

    it.each([
      ["cmd var -m ", SuggestionFlag.Args, optionArgs(vrd, "-m")[0], ""],
      ["cmd var -o ", SuggestionFlag.Any, vrd.args[0], ""],
      ["cmd var -v ", SuggestionFlag.Args, optionArgs(vrd, "-v")[0], ""],
      ["cmd var -n ", SuggestionFlag.Any, vrd.args[0], ""],
      ["cmd var -marg", SuggestionFlag.Args, optionArgs(vrd, "-m")[0], "arg"],
      ["cmd var -oarg", SuggestionFlag.Args, optionArgs(vrd, "-o")[0], "arg"],
    ])("variadic subcommand: %p", testVar);

    it.each([
      ["cmd opt -m ", SuggestionFlag.Args, optionArgs(opt, "-m")[0], ""],
      ["cmd opt -o ", SuggestionFlag.Any, opt.args[0], ""],
      ["cmd opt -v ", SuggestionFlag.Args, optionArgs(opt, "-v")[0], ""],
      ["cmd opt -n ", SuggestionFlag.Any, opt.args[0], ""],
      ["cmd opt -marg", SuggestionFlag.Args, optionArgs(opt, "-m")[0], "arg"],
      ["cmd opt -oarg", SuggestionFlag.Args, optionArgs(opt, "-o")[0], "arg"],
    ])("optional subcommand: %p", testOpt);

    it.each([
      ["cmd -m ", SuggestionFlag.Args, optionArgs(cmd, "-m")[0], ""],
      ["cmd -o ", SuggestionFlag.Any, optionArgs(cmd, "-o")[0], ""],
      ["cmd -v ", SuggestionFlag.Args, optionArgs(cmd, "-v")[0], ""],
      ["cmd -n ", SuggestionFlag.Any, null, ""],
      ["cmd -marg", SuggestionFlag.Args, optionArgs(cmd, "-m")[0], "arg"],
      ["cmd -oarg", SuggestionFlag.Args, optionArgs(cmd, "-o")[0], "arg"],
    ])("no arg subcommand: %p", testCmd);
  });

  describe("exceptions", () => {
    beforeEach(() => {
      // Mock loading of the original cmd spec, and then the loaded spec.
      resetCaches();
      (loadSubcommandCached as Mock).mockResolvedValueOnce(cmd);
    });
    // Empty command
    it("empty command", async () => {
      await expect(parseArguments(null, emptyContext)).rejects.toThrow();
    });

    it("single, non-dotslash tokenArray", async () => {
      const command = getCommand("cmd", {});
      const result = await parseArguments(command, emptyContext);
      expect(result.completionObj.name).toContain("firstTokenSpec");
    });

    // Can't reconcile tokens with spec.
    it("invalid token", async () => {
      const command = getCommand("cmd notASubcommand ", {});
      await expect(parseArguments(command, emptyContext)).rejects.toThrow();
    });
  });

  describe("loadSpec and specialArgs", () => {
    const specToLoad: Internal.Subcommand = {
      ...baseSpec,
      name: ["specToLoad"],
      args: [{ generators: [] }],
    };
    beforeEach(() => {
      // Mock loading of the original cmd spec, and then the loaded spec.
      resetCaches();
      (loadSubcommandCached as Mock)
        .mockResolvedValueOnce(cmd)
        .mockResolvedValueOnce(specToLoad);
    });

    it("loadSpec", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("cmd loadSpec ", {});
      const result = await parseArguments(command, context);

      const location = findSubcommand(cmd, "loadSpec").loadSpec;

      expect(loadSubcommandCached).toBeCalledTimes(2);
      if (Array.isArray(location)) {
        expect(loadSubcommandCached).toHaveBeenLastCalledWith(
          location[0],
          context,
          logger,
        );
      } else {
        expect(false);
      }

      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.currentArg).toStrictEqual(specToLoad.args[0]);
      expect(result.completionObj).toBe(specToLoad);
    });

    it("dotslash", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("./test", {});
      await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(1);
      expect(loadSubcommandCached).toHaveBeenLastCalledWith(
        {
          name: "dotslash",
          type: SpecLocationSource.GLOBAL,
        },
        context,
        logger,
      );
    });

    it("bin/console", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("bin/console", {});
      await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(1);
      expect(loadSubcommandCached).toHaveBeenLastCalledWith(
        {
          name: "php/bin-console",
          type: SpecLocationSource.GLOBAL,
        },
        context,
        logger,
      );
    });

    it("relative path first token", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("./dir/test ", {});
      await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(1);
      expect(loadSubcommandCached).toHaveBeenLastCalledWith(
        {
          name: "test",
          type: SpecLocationSource.LOCAL,
          path: `${cwd}/dir/`,
        },
        context,
        logger,
      );
    });

    it("load spec syntactic sugar", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("cmd module moduleName ", {});

      const result = await parseArguments(command, context);
      const { isModule } = makeArray(
        findSubcommand(cmd, "module").args || [],
      )[0];
      const moduleName = `${isModule}moduleName`;

      expect(loadSubcommandCached).toBeCalledTimes(2);
      // Access mock calls to check deep equals (can't use toBeCalledWith)
      expect(loadSubcommandCached).toHaveBeenLastCalledWith(
        {
          name: moduleName,
          type: SpecLocationSource.GLOBAL,
        },
        context,
        logger,
      );

      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");

      expect(result.currentArg).toStrictEqual(specToLoad.args[0]);
      expect(result.completionObj).toBe(specToLoad);
    });
  });

  describe("recursive loadSpec", () => {
    beforeEach(() => {
      // Mock loading of the original cmd spec, and then the loaded spec.
      resetCaches();
      (loadSubcommandCached as Mock)
        .mockResolvedValueOnce(cmd)
        .mockResolvedValueOnce(cmd.subcommands.recursiveLoadSpecNested)
        .mockResolvedValueOnce(cmd.subcommands.recursiveLoadSpecNestedNested);
    });

    it("should load the correct spec when nested loadSpec are used", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("cmd recursiveLoadSpec nestedSubcommand ", {});
      const result = await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(3);
      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.annotations.length).toBe(2);
      expect(result.completionObj).toBe(
        cmd.subcommands.recursiveLoadSpecNestedNested,
      );
    });

    it("should load the correct spec when nested loadSpec are used (1)", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand(
        "cmd recursiveLoadSpec nestedSubcommand doubleNestedSubcommand ",
        {},
      );
      const result = await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(3);
      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.annotations.length).toBe(3);
      expect(result.currentArg).toStrictEqual(
        cmd.subcommands.recursiveLoadSpecNestedNested.subcommands
          .doubleNestedSubcommand.args[0],
      );
      expect(result.completionObj).toBe(
        cmd.subcommands.recursiveLoadSpecNestedNested.subcommands
          .doubleNestedSubcommand,
      );
    });
  });

  describe("isCommand", () => {
    beforeEach(() => {
      // Mock loading of the original cmd spec, and then the loaded spec.
      resetCaches();
      (loadSubcommandCached as Mock)
        .mockResolvedValue(cmd.subcommands.sudo)
        .mockResolvedValueOnce(cmd);
    });

    it("should load the correct spec with one isCommand", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("cmd sudo ", {});
      const result = await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(1);
      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.annotations.length).toBe(3);
      expect(result.completionObj).toBe(cmd.subcommands.sudo);
    });

    it("should load the correct spec with nested isCommand", async () => {
      const cwd = "/path/to/cwd";
      const context = { ...emptyContext, currentWorkingDirectory: cwd };

      const command = getCommand("cmd sudo sudo sudo sudo ", {});
      const result = await parseArguments(command, context);

      expect(loadSubcommandCached).toBeCalledTimes(4);
      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.annotations.length).toBe(2);
      expect(result.currentArg).toStrictEqual(cmd.subcommands.sudo.args[0]);
      expect(result.completionObj).toBe(cmd.subcommands.sudo);
    });
  });

  describe("generateSpec", () => {
    // Mock loading of the original cmd spec.
    beforeEach(() => {
      resetCaches();
      (loadSubcommandCached as Mock).mockResolvedValueOnce(cmd);
    });

    it("works", async () => {
      const command = getCommand("cmd generateSpec ", {});

      const result = await parseArguments(command, emptyContext);

      const subcommand = findSubcommand(cmd, "generateSpec");
      const { generateSpec } = subcommand;

      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");

      // We mutate completionObj for generateSpec so check with toEqual instead of toBe.
      const newSpec = await generateSpec!(
        ["cmd", "generateSpec", ""],
        async () => ({
          status: 0,
          stderr: "",
          stdout: "",
        }),
      );
      const spec = convertSubcommand(newSpec, initializeDefault);
      expect(result.completionObj).toEqual({
        ...subcommand,
        subcommands: spec.subcommands,
        args: spec.args,
        options: { ...subcommand.options, ...spec.options },
      });
      expect(result.currentArg).toEqual(spec.args[0]);
    });

    it("works when generate spec throws", async () => {
      // Pass option so generateSpec throws
      const command = getCommand("cmd -mmsg generateSpec ", {});
      const result = await parseArguments(command, emptyContext);

      expect(result.suggestionFlags).toBe(SuggestionFlag.Any);
      expect(result.searchTerm).toBe("");
      expect(result.completionObj).toEqual(findSubcommand(cmd, "generateSpec"));
      expect(result.currentArg).toEqual(null);
    });
  });

  afterEach(() => {
    (loadSubcommandCached as Mock).mockRestore();
  });
});
