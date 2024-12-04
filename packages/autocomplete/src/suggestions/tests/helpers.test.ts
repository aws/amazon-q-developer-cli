global.fig = {
  ...global.fig,
  settings: {},
};

const originalGlobalFig = global.fig;

import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  getNameMatch,
  getExactNameMatch,
  getCommonSuggestionPrefix,
  getQueryTermForSuggestion,
  bestScoreMatch,
} from "../helpers";

const foo: Suggestion = {
  name: "foo",
  type: "subcommand",
};

const fooOrFoobar: Suggestion = {
  name: ["foo", "FooBar"],
  type: "subcommand",
};

describe("getNameMatch", () => {
  beforeEach(() => {
    global.fig = {
      ...global.fig,
      settings: {},
    };
  });

  afterEach(() => {
    global.fig = originalGlobalFig;
  });

  it("should match", () => {
    expect(getNameMatch(foo, "foo", false, false)).toEqual("foo");
    expect(getNameMatch(foo, "", false, false)).toEqual("foo");
    expect(getNameMatch(fooOrFoobar, "", false, false)).toEqual("foo");
  });

  it("should match with array", () => {
    expect(getNameMatch(fooOrFoobar, "foo", false, false)).toEqual("foo");
    expect(getNameMatch(fooOrFoobar, "foob", false, false)).toEqual("FooBar");
  });

  it("should not match", () => {
    expect(getNameMatch(foo, "bar", false, false)).toEqual(undefined);
  });

  it("should match with different case", () => {
    expect(getNameMatch(foo, "FOO", false, false)).toEqual("foo");
  });

  it("should not match if contains", () => {
    expect(getNameMatch(fooOrFoobar, "bar", false, false)).toEqual(undefined);
  });

  describe("with fuzzy search", () => {
    beforeEach(() => {
      global.fig = {
        ...global.fig,
        settings: {
          "autocomplete.fuzzySearch": true,
        },
      };
    });

    afterEach(() => {
      global.fig = originalGlobalFig;
    });

    it("should match", () => {
      expect(getNameMatch(foo, "foo", true, false)).toEqual("foo");
      expect(getNameMatch(fooOrFoobar, "foo", true, false)).toEqual("foo");
      expect(getNameMatch(fooOrFoobar, "foobar", true, false)).toEqual(
        "FooBar",
      );
      expect(getNameMatch(foo, "", true, false)).toEqual("foo");
      expect(getNameMatch(fooOrFoobar, "", true, false)).toEqual("foo");
    });

    it("should match with different case", () => {
      expect(getNameMatch(fooOrFoobar, "OBA", true, false)).toEqual("FooBar");
      expect(getNameMatch(fooOrFoobar, "oBa", true, false)).toEqual("FooBar");
    });

    it("should match if contains", () => {
      expect(getNameMatch(fooOrFoobar, "oo", true, false)).toEqual("foo");
      expect(getNameMatch(fooOrFoobar, "oba", true, false)).toEqual("FooBar");
    });

    it("should not match", () => {
      expect(getNameMatch(foo, "bar", true, false)).toEqual(undefined);
    });
  });

  describe("with verbose suggestion preferred", () => {
    beforeEach(() => {
      global.fig = {
        ...global.fig,
        settings: {
          "autocomplete.preferVerboseSuggestions": true,
        },
      };
    });

    afterEach(() => {
      global.fig = originalGlobalFig;
    });
    it("should match with array", () => {
      expect(getNameMatch(fooOrFoobar, "fo", false, true)).toEqual("FooBar");
      expect(getNameMatch(fooOrFoobar, "foob", false, true)).toEqual("FooBar");
    });

    it("should not match", () => {
      expect(getNameMatch(foo, "bar", false, true)).toEqual(undefined);
    });

    it("should match if contains (fuzzy)", () => {
      expect(getNameMatch(fooOrFoobar, "bar", true, true)).toEqual("FooBar");
      expect(getNameMatch(fooOrFoobar, "fo", true, true)).toEqual("FooBar");
    });
  });
});

describe("getExactMatch", () => {
  it("should return exact match", () => {
    expect(getExactNameMatch(foo, "foo")).toEqual("foo");
    expect(getExactNameMatch(fooOrFoobar, "foo")).toEqual("foo");
    expect(getExactNameMatch(fooOrFoobar, "foobar")).toEqual("FooBar");

    expect(getExactNameMatch(foo, "FOO")).toEqual("foo");
    expect(getExactNameMatch(fooOrFoobar, "FoObAr")).toEqual("FooBar");
  });

  it("should not return exact match", () => {
    expect(getExactNameMatch(foo, "foobar")).toEqual(undefined);
    expect(getExactNameMatch(foo, "fo")).toEqual(undefined);
    expect(getExactNameMatch(fooOrFoobar, "")).toEqual(undefined);
    expect(getExactNameMatch(fooOrFoobar, "foob")).toEqual(undefined);
  });
});

describe("getCommonSuggestionPrefix", () => {
  const suggestions: Suggestion[] = [
    { name: "no_type" },
    { name: "commit", type: "subcommand" },
    { name: "com", type: "subcommand" },
    { name: "../", type: "folder" },
    { name: ["foobar", "bar"], type: "file" },
    { name: "foo/", type: "folder" },
    { name: "foo", type: "file" },
    { name: "bar", type: "auto-execute" },
    { type: "auto-execute" },
    { name: "bar", type: "arg" },
    { type: "arg" },
  ];

  it("returns null without type", () => {
    expect(getCommonSuggestionPrefix(0, suggestions)).toBe(null);
    expect(getCommonSuggestionPrefix(100, suggestions)).toBe(null);
  });

  it("filters out elements of different types", () => {
    expect(getCommonSuggestionPrefix(1, suggestions)).toBe("com");
    expect(getCommonSuggestionPrefix(2, suggestions)).toBe("com");
    for (let i = 4; i < 7; i += 1) {
      expect(getCommonSuggestionPrefix(i, suggestions)).toBe("foo");
    }
  });

  it("returns null for auto-execute type", () => {
    expect(getCommonSuggestionPrefix(7, suggestions)).toBe(null);
  });

  it("works with suggestions without names", () => {
    expect(getCommonSuggestionPrefix(9, suggestions)).toBe("");
  });
});

describe("getQueryTermForSuggestion", () => {
  const withStringQueryTerm = {
    generator: {
      getQueryTerm: "/",
    },
  };
  const withGetQueryTerm = {
    generator: {
      getQueryTerm: (term: string) => `foo${term}`,
    } as Fig.Generator,
  };

  const noQueryTerm = {};

  it("works with query term", () => {
    expect(getQueryTermForSuggestion(withStringQueryTerm, "~/foo")).toBe("foo");
    expect(getQueryTermForSuggestion(withStringQueryTerm, "foo")).toBe("foo");
  });

  it("works with query term fn", () => {
    expect(getQueryTermForSuggestion(withGetQueryTerm, "~/foo")).toBe(
      "foo~/foo",
    );
    expect(getQueryTermForSuggestion(withGetQueryTerm, "foo")).toBe("foofoo");
  });

  it("works with no query term", () => {
    expect(getQueryTermForSuggestion(noQueryTerm, "foo")).toBe("foo");
    expect(getQueryTermForSuggestion(noQueryTerm, "bar")).toBe("bar");
    expect(getQueryTermForSuggestion(noQueryTerm, "")).toBe("");
  });
});

describe("bestScoreMatch", () => {
  it("should return null if everything is null", () => {
    expect(bestScoreMatch([null, null, null])).toBeNull();
  });

  it("should return the best score found", () => {
    expect(
      bestScoreMatch([
        null,
        { score: 10 },
        null,
        { score: 12 },
        { score: 8 },
        null,
      ]),
    ).toStrictEqual({ score: 12 });
  });
});
