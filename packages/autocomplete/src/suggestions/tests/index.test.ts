import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import { SETTINGS } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import * as settings from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import {
  MockInstance,
  afterAll,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  getStaticArgSuggestions,
  filterSuggestions,
  deduplicateSuggestions,
  isTemplateSuggestion,
} from "..";

describe("isTemplateSuggestion", () => {
  it("should return true when object has context", () => {
    const suggestion: Fig.TemplateSuggestion = {
      context: { templateType: "folders" },
    };
    expect(isTemplateSuggestion(suggestion)).toBe(true);
  });

  it("should return false when object has no context", () => {
    const suggestion: Fig.Suggestion = { name: "foo" };
    expect(isTemplateSuggestion(suggestion)).toBe(false);
  });
});

describe("getStaticArgSuggestions", () => {
  it("handles string arg suggestions", () => {
    expect(
      getStaticArgSuggestions({
        generators: [],
        suggestions: ["arg"],
        isDangerous: true,
      }),
    ).toEqual([
      {
        name: "arg",
        type: "arg",
        isDangerous: true,
      },
    ]);
  });

  it("handles object arg suggestions", () => {
    expect(
      getStaticArgSuggestions({
        suggestions: [
          {
            name: "sub",
            type: "subcommand",
            isDangerous: false,
          },
        ],
        generators: [],
        isDangerous: true,
      }),
    ).toEqual([
      {
        name: "sub",
        type: "subcommand",
        isDangerous: false,
      },
    ]);
  });

  it("returns empty array on empty suggestions object", () => {
    expect(getStaticArgSuggestions({ generators: [] })).toEqual([]);
    expect(
      getStaticArgSuggestions({ suggestions: [], generators: [] }),
    ).toEqual([]);
  });
});

describe("filterSuggestions", () => {
  it("should add auto-execute when searchTerm is `.`", () => {
    const suggestions: Suggestion[] = [
      { name: ".eslintrc", type: "file" },
      { name: "dirB/", type: "folder" },
    ];
    const result = filterSuggestions(suggestions, ".", false, false);
    expect(result).toEqual([
      {
        description: "Enter the current directory",
        insertValue: "\n",
        name: ".",
        originalType: "folder",
        type: "auto-execute",
      },
      expect.objectContaining({
        name: ".eslintrc",
        type: "file",
      }),
    ]);
  });

  it("should add auto-execute to matching suggestion when it is a folder missing trailing slash", () => {
    const suggestions: Suggestion[] = [
      { name: "dirA/", type: "folder" },
      { name: "dirB/", type: "folder" },
    ];
    const result = filterSuggestions(suggestions, "dirA", false, false);
    expect(result).toEqual([
      expect.objectContaining({
        name: "dirA",
        type: "auto-execute",
        description: "folder",
      }),
      { name: "dirA/", type: "folder" },
    ]);
  });

  it("should add auto-execute to matching suggestion", () => {
    const suggestions: Suggestion[] = [
      { name: "foo", description: "Some description" },
      { name: "foobar" },
      { name: "bar" },
    ];
    const result = filterSuggestions(suggestions, "foo", false, false);
    expect(result).toEqual([
      expect.objectContaining({
        description: "Some description",
        name: "foo",
        type: "auto-execute",
      }),
      {
        description: "Some description",
        name: "foo",
      },
      expect.objectContaining({ name: "foobar" }),
    ]);
  });

  it("should not add current token to matching suggestion when `suggestCurrentToken: false`", () => {
    const suggestions: Suggestion[] = [
      { name: "foo", description: "Some description" },
      { name: "foobar" },
      { name: "bar" },
    ];
    const result = filterSuggestions(suggestions, "fo", false, false);
    expect(result).toEqual([
      { name: "foo", description: "Some description" },
      expect.objectContaining({ name: "foobar" }),
    ]);
  });

  describe("`suggestCurrentToken: true`", () => {
    it("should add auto-execute suggestion when searchTerm matches partially some of the suggestions", () => {
      const suggestions: Suggestion[] = [{ name: "foo" }, { name: "bar" }];
      const result = filterSuggestions(suggestions, "fo", false, true);
      expect(result).toEqual([
        expect.objectContaining({
          name: "fo",
          type: "auto-execute",
          description: "Enter the current argument",
        }),
        { name: "foo" },
      ]);
    });
    it("should add auto-execute suggestion when searchTerm matches partially some of the suggestions (folder suggestion matching)", () => {
      const suggestions: Suggestion[] = [
        { name: "dirA/", type: "folder" },
        { name: "dirB/", type: "folder" },
      ];
      const result = filterSuggestions(suggestions, "dirA", false, true);
      expect(result).toEqual([
        expect.objectContaining({
          name: "dirA",
          type: "auto-execute",
          description: "folder",
        }),
        { name: "dirA/", type: "folder" },
      ]);
    });

    it("should not add auto-execute suggestion when searchTerm does not match any of the suggestions", () => {
      const suggestions: Suggestion[] = [{ name: "foo" }, { name: "bar" }];
      const result = filterSuggestions(suggestions, "bo", false, true);
      expect(result).toEqual([]);
    });
  });

  describe("should not add auto-execute when setting disables it", () => {
    // export declare const getSetting: <T = unknown>(
    //   key: SETTINGS,
    //   defaultValue?: any,
    // ) => T;
    let spy: MockInstance;
    beforeAll(() => {
      spy = vi.spyOn(settings, "getSetting");
      spy.mockImplementation((arg) => {
        if (arg == SETTINGS.HIDE_AUTO_EXECUTE_SUGGESTION) return true;
        return undefined;
      });
    });

    afterAll(() => {
      spy.mockRestore();
    });

    it("to the matching suggestion", () => {
      const suggestions: Suggestion[] = [
        { name: "foo", description: "Some description" },
        { name: "foobar" },
        { name: "bar" },
      ];
      const result = filterSuggestions(suggestions, "foo", false, false);
      expect(result).toEqual([
        expect.not.objectContaining({
          type: "auto-execute",
        }),
        { name: "foobar" },
      ]);
    });

    it("when `suggestCurrentToken: false`", () => {
      const suggestions: Suggestion[] = [{ name: "foo" }, { name: "bar" }];
      const result = filterSuggestions(suggestions, "fo", false, false);
      expect(result).toEqual([
        {
          name: "foo",
        },
      ]);
    });

    it("when searchTerm is `.`", () => {
      const suggestions: Suggestion[] = [{ name: "..", type: "folder" }];
      const result = filterSuggestions(suggestions, ".", false, false);
      expect(result).toEqual([
        expect.objectContaining({
          name: "..",
        }),
      ]);
    });
  });

  it("should remove suggestions when searchTerm does not match any of the suggestions", () => {
    const suggestions: Suggestion[] = [{ name: "foo" }, { name: "bar" }];
    const result = filterSuggestions(suggestions, "bo", false, false);
    expect(result).toEqual([]);
  });
});

describe("deduplicateSuggestions", () => {
  it("should remove duplicate suggestions by name", () => {
    const suggestions: Suggestion[] = [
      { name: "foo" },
      { name: "bar", icon: "icon1" },
      { name: "baz", icon: "icon1" },
      { name: "foo", icon: "icon" },
      { name: "bar", icon: "icon2" },
      { name: "baz", icon: "icon1" },
    ];
    const expected: Suggestion[] = [
      { name: "foo" },
      { name: "bar", icon: "icon1" },
      { name: "baz", icon: "icon1" },
    ];
    expect(deduplicateSuggestions(suggestions, ["name"])).toStrictEqual(
      expected,
    );
  });

  it("should remove duplicate suggestions by multiple fields", () => {
    const suggestions: Suggestion[] = [
      { name: "foo" },
      { name: "bar", icon: "icon1" },
      { name: "baz", icon: "icon1" },
      { name: "foo", icon: "icon" },
      { name: "bar", icon: "icon2" },
      { name: "baz", icon: "icon1" },
    ];
    const expected: Suggestion[] = [
      { name: "foo" },
      { name: "bar", icon: "icon1" },
      { name: "baz", icon: "icon1" },
      { name: "foo", icon: "icon" },
      { name: "bar", icon: "icon2" },
    ];
    expect(deduplicateSuggestions(suggestions, ["name", "icon"])).toStrictEqual(
      expected,
    );
  });

  it("should remove duplicate suggestions by nested fields", () => {
    const args: Suggestion["args"] = [
      { generators: [{ custom: async () => [] }] },
    ];
    const suggestions: Suggestion[] = [
      { name: "a", displayName: "a.", icon: "icon1", args },
      { name: "a", displayName: "a.", icon: "icon2", args },
      { name: "a", displayName: "b.", args },
      { name: "c", args: [{ name: "foo", generators: [] }] },
      { name: "c", args: [{ name: "foo", generators: [] }] },
    ];
    const expected: Suggestion[] = [
      { name: "a", displayName: "a.", icon: "icon1", args },
      { name: "a", displayName: "b.", args },
      { name: "c", args: [{ name: "foo", generators: [] }] },
    ];
    expect(
      deduplicateSuggestions(suggestions, ["name", "displayName", "args"]),
    ).toStrictEqual(expected);
  });
});
