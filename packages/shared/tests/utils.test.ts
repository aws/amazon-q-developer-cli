import { describe, expect, it } from "vitest";
import {
  makeArray,
  makeArrayIfExists,
  longestCommonPrefix,
  compareNamedObjectsAlphabetically,
  fieldsAreEqual,
} from "../src/utils";

describe("fieldsAreEqual", () => {
  it("should return immediately if two values are the same", () => {
    expect(fieldsAreEqual("hello", "hello", [])).toEqual(true);
    expect(fieldsAreEqual("hello", "hell", [])).toEqual(false);
    expect(fieldsAreEqual(1, 1, ["valueOf"])).toEqual(true);
    expect(fieldsAreEqual(null, null, [])).toEqual(true);
    expect(fieldsAreEqual(null, undefined, [])).toEqual(false);
    expect(fieldsAreEqual(undefined, undefined, [])).toEqual(true);
    expect(fieldsAreEqual(null, "hello", [])).toEqual(false);
    expect(fieldsAreEqual(100, null, [])).toEqual(false);
    expect(fieldsAreEqual({}, {}, [])).toEqual(true);
    expect(
      fieldsAreEqual(
        () => {},
        () => {},
        [],
      ),
    ).toEqual(false);
  });

  it("should return true if fields are equal", () => {
    const fn = () => {};
    expect(
      fieldsAreEqual(
        {
          a: "hello",
          b: 100,
          c: undefined,
          d: false,
          e: fn,
          f: { fa: true, fb: { fba: true } },
          g: null,
        },
        {
          a: "hello",
          b: 100,
          c: undefined,
          d: false,
          e: fn,
          f: { fa: true, fb: { fba: true } },
          g: null,
        },
        ["a", "b", "c", "d", "e", "f", "g"],
      ),
    ).toEqual(true);
    expect(fieldsAreEqual({ a: {} }, { a: {} }, ["a"])).toEqual(true);
  });

  it("should return false if any field is not equal or fields are not specified", () => {
    expect(fieldsAreEqual({ a: null }, { a: {} }, ["a"])).toEqual(false);
    expect(fieldsAreEqual({ a: undefined }, { a: "hello" }, ["a"])).toEqual(
      false,
    );
    expect(fieldsAreEqual({ a: false }, { a: true }, ["a"])).toEqual(false);
    expect(
      fieldsAreEqual(
        { a: { b: { c: "hello" } } },
        { a: { b: { c: "hell" } } },
        ["a"],
      ),
    ).toEqual(false);
    expect(fieldsAreEqual({ a: "true" }, { b: "true" }, [])).toEqual(false);
  });
});

describe("makeArray", () => {
  it("should transform an object into an array", () => {
    expect(makeArray(true)).toEqual([true]);
  });

  it("should not transform arrays with one value", () => {
    expect(makeArray([true])).toEqual([true]);
  });

  it("should not transform arrays with multiple values", () => {
    expect(makeArray([true, false])).toEqual([true, false]);
  });
});

describe("makeArrayIfExists", () => {
  it("works", () => {
    expect(makeArrayIfExists(null)).toEqual(null);
    expect(makeArrayIfExists(undefined)).toEqual(null);
    expect(makeArrayIfExists("a")).toEqual(["a"]);
    expect(makeArrayIfExists(["a"])).toEqual(["a"]);
  });
});

describe("longestCommonPrefix", () => {
  it("should return the shared match", () => {
    expect(longestCommonPrefix(["foo", "foo bar", "foo hello world"])).toEqual(
      "foo",
    );
  });

  it("should return nothing if not all items starts by the same chars", () => {
    expect(longestCommonPrefix(["foo", "foo bar", "hello world"])).toEqual("");
  });
});

describe("compareNamedObjectsAlphabetically", () => {
  it("should return 1 to sort alphabetically z against b for string", () => {
    expect(compareNamedObjectsAlphabetically("z", "b")).toBeGreaterThan(0);
  });

  it("should return 1 to sort alphabetically z against b for object with name", () => {
    expect(
      compareNamedObjectsAlphabetically({ name: "z" }, { name: "b" }),
    ).toBeGreaterThan(0);
  });

  it("should return 1 to sort alphabetically c against x for object with name", () => {
    expect(
      compareNamedObjectsAlphabetically({ name: "c" }, { name: "x" }),
    ).toBeLessThan(0);
  });

  it("should return 1 to sort alphabetically z against b for object with name array", () => {
    expect(
      compareNamedObjectsAlphabetically({ name: ["z"] }, { name: ["b"] }),
    ).toBeGreaterThan(0);
  });

  it("should return 1 to sort alphabetically c against x for object with name array", () => {
    expect(
      compareNamedObjectsAlphabetically({ name: ["c"] }, { name: ["x"] }),
    ).toBeLessThan(0);
  });
});
