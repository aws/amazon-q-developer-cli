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
import { getCustomSuggestions } from "../customSuggestionsGenerator";
import * as helpers from "../helpers";
import { GeneratorContext } from "../helpers";

const context: GeneratorContext = {
  annotations: [] as Annotation[],
  tokenArray: [] as string[],
  currentWorkingDirectory: "/",
  currentProcess: "zsh",
  sshPrefix: "",
  searchTerm: "",
  environmentVariables: {},
};

describe("getCustomSuggestions", () => {
  let runCachedGenerator: MockInstance;

  beforeAll(() => {
    runCachedGenerator = vi.spyOn(helpers, "runCachedGenerator");
  });

  afterEach(() => {
    runCachedGenerator.mockClear();
  });

  it("should return the result", async () => {
    expect(
      await getCustomSuggestions(
        {
          custom: () => Promise.resolve([{ name: "hello" }, { name: "world" }]),
        },
        context,
      ),
    ).toEqual([
      { name: "hello", type: "arg" },
      { name: "world", type: "arg" },
    ]);
  });

  it("should return the result and infer type", async () => {
    expect(
      await getCustomSuggestions(
        {
          custom: () =>
            Promise.resolve([
              { name: "hello", type: "shortcut" },
              { name: "world", type: "folder" },
            ]),
        },
        context,
      ),
    ).toEqual([
      { name: "hello", type: "shortcut" },
      { name: "world", type: "folder" },
    ]);
  });

  it("should call runCachedGenerator", async () => {
    await getCustomSuggestions(
      {
        custom: () => Promise.resolve([{ name: "hello" }, { name: "world" }]),
      },
      context,
    );

    expect(runCachedGenerator).toHaveBeenCalled();
  });

  it("should call runCachedGenerator and the custom function", async () => {
    const custom = vi
      .fn()
      .mockResolvedValue([{ name: "hello" }, { name: "world" }]);

    await getCustomSuggestions({ custom }, context);

    expect(runCachedGenerator).toHaveBeenCalled();
    expect(custom).toHaveBeenCalled();

    custom.mockClear();
  });
});
