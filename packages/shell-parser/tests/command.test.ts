import { describe, expect, it } from "vitest";
import { getCommand, Command } from "../src/command";

describe("getCommand", () => {
  const aliases = {
    woman: "man",
    quote: "'q'",
    g: "git",
  };
  const expectText = (command: Command | null) =>
    expect(command?.tokens.map((token) => token.text) ?? []);

  it("works without matching aliases", () => {
    expectText(getCommand("git co ", {})).toEqual(["git", "co", ""]);
    expectText(getCommand("git co ", aliases)).toEqual(["git", "co", ""]);
    expectText(getCommand("woman ", {})).toEqual(["woman", ""]);
    expectText(getCommand("another string ", aliases)).toEqual([
      "another",
      "string",
      "",
    ]);
  });

  it("works with regular aliases", () => {
    // Don't change a single token.
    expectText(getCommand("woman", aliases)).toEqual(["woman"]);
    // Change first token if length > 1.
    expectText(getCommand("woman ", aliases)).toEqual(["man", ""]);
    // Don't change later tokens.
    expectText(getCommand("man woman ", aliases)).toEqual(["man", "woman", ""]);
    // Handle quotes
    expectText(getCommand("quote ", aliases)).toEqual(["q", ""]);
  });
});
