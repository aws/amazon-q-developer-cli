import * as apiBindingsWrappers from "@aws/amazon-q-developer-cli-api-bindings-wrappers/executeCommand";

import { Annotation } from "@aws/amazon-q-developer-cli-autocomplete-parser";
import {
  MockInstance,
  afterEach,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import * as helpers from "../helpers";
import { GeneratorContext } from "../helpers";
import { getScriptSuggestions } from "../scriptSuggestionsGenerator";

vi.mock(
  "@aws/amazon-q-developer-cli-api-bindings-wrappers/src/executeCommand",
  async () =>
    vi.importActual(
      "@aws/amazon-q-developer-cli-api-bindings-wrappers/src/executeCommand",
    ),
);

const context: GeneratorContext = {
  annotations: [] as Annotation[],
  tokenArray: [] as string[],
  currentWorkingDirectory: "/",
  sshPrefix: "",
  currentProcess: "zsh",
  searchTerm: "",
  environmentVariables: {},
};

describe.todo("getScriptSuggestions", () => {
  let executeCommand: MockInstance;

  beforeAll(() => {
    vi.spyOn(helpers, "runCachedGenerator");
    executeCommand = vi
      .spyOn(apiBindingsWrappers, "executeCommandTimeout")
      .mockResolvedValue({ status: 0, stdout: "a/\nx\nc/\nl", stderr: "" });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("should return empty suggestions if no script in generator", async () => {
    expect(await getScriptSuggestions({ script: [] }, context, 5000)).toEqual(
      [],
    );
  });

  it("should return empty suggestions if no splitOn or postProcess", async () => {
    expect(
      await getScriptSuggestions({ script: ["ascript"] }, context, 5000),
    ).toEqual([]);
  });

  it("should return the result with splitOn", async () => {
    expect(
      await getScriptSuggestions(
        { script: ["ascript"], splitOn: "\n" },
        context,
        5000,
      ),
    ).toEqual([
      { insertValue: "a/", isDangerous: undefined, name: "a/", type: "arg" },
      { insertValue: "x", isDangerous: undefined, name: "x", type: "arg" },
      { insertValue: "c/", isDangerous: undefined, name: "c/", type: "arg" },
      { insertValue: "l", isDangerous: undefined, name: "l", type: "arg" },
    ]);
  });

  it("should return the result with postProcess", async () => {
    const postProcess = vi
      .fn()
      .mockReturnValue([{ name: "hello" }, { name: "world" }]);

    expect(
      await getScriptSuggestions(
        { script: ["ascript"], postProcess },
        context,
        5000,
      ),
    ).toEqual([
      { name: "hello", type: "arg" },
      { name: "world", type: "arg" },
    ]);
    expect(postProcess).toHaveBeenCalledWith("a/\nx\nc/\nl", []);
  });

  it("should return the result with postProcess and infer type", async () => {
    const postProcess = vi.fn().mockReturnValue([
      { name: "hello", type: "auto-execute" },
      { name: "world", type: "folder" },
    ]);

    expect(
      await getScriptSuggestions(
        { script: ["ascript"], postProcess },
        context,
        5000,
      ),
    ).toEqual([
      { name: "hello", type: "auto-execute" },
      { name: "world", type: "folder" },
    ]);
    expect(postProcess).toHaveBeenCalledWith("a/\nx\nc/\nl", []);
  });

  it("should call script if provided", async () => {
    const script = vi.fn().mockReturnValue("myscript");
    await getScriptSuggestions({ script }, context, 5000);
    expect(script).toHaveBeenCalledWith([]);
  });

  // it("should call runCachedGenerator", async () => {
  //   await getScriptSuggestions({ script: "ascript" }, context, 5000);
  //   expect(runCachedGenerator).toHaveBeenCalled();
  // });

  it("should call executeCommand", async () => {
    await getScriptSuggestions({ script: ["ascript"] }, context, 5000);
    expect(executeCommand).toHaveBeenCalledWith(
      {
        args: [],
        command: "ascript",
        cwd: "/",
      },
      5000,
    );
  });

  it("should call executeCommand with 'spec-specified' timeout", async () => {
    await getScriptSuggestions(
      { script: ["ascript"], scriptTimeout: 6000 },
      context,
      5000,
    );
    expect(executeCommand).toHaveBeenCalledWith(
      { args: [], command: "ascript", cwd: "/" },
      6000,
    );
  });

  it("should use the greatest between the settings timeout and the spec defined one", async () => {
    await getScriptSuggestions(
      { script: ["ascript"], scriptTimeout: 3500 },
      context,
      7000,
    );
    expect(executeCommand).toHaveBeenCalledWith(
      { args: [], command: "ascript", cwd: "/" },
      7000,
    );
  });

  it("should call executeCommand without timeout when the user defined ones are negative", async () => {
    await getScriptSuggestions(
      { script: ["ascript"], scriptTimeout: -100 },
      context,
      -1000,
    );
    expect(executeCommand).toHaveBeenCalledWith(
      { args: [], command: "ascript", cwd: "/" },
      undefined,
    );
  });

  it("should call executeCommand with settings timeout when no 'spec-specified' one is defined", async () => {
    await getScriptSuggestions({ script: ["ascript"] }, context, 6000);
    expect(executeCommand).toHaveBeenCalledWith(
      { args: [], command: "ascript", cwd: "/" },
      6000,
    );
  });

  describe("deprecated sshPrefix", () => {
    it("should call executeCommand ignoring ssh", async () => {
      await getScriptSuggestions(
        { script: ["ascript"] },
        {
          ...context,
          sshPrefix: "ssh -i blabla",
        },
        5000,
      );

      expect(executeCommand).toHaveBeenCalledWith(
        { args: [], command: "ascript", cwd: "/" },
        5000,
      );
    });
  });
});
