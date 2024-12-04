import * as wrappers from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import * as loadHelpers from "../src/loadHelpers";
import { vi, describe, beforeEach, afterEach, it, expect } from "vitest";

const { getVersionFromFullFile } = loadHelpers;

const specData = {
  getVersionCommand: vi.fn().mockReturnValue(Promise.resolve("1.0.0")),
  default: () => {},
};

describe("test `getVersionFromFullFile`", () => {
  beforeEach(() => {
    vi.spyOn(loadHelpers, "getCachedCLIVersion").mockReturnValue(null);
  });
  afterEach(() => {
    vi.clearAllMocks();
  });
  it("missing `getVersionCommand` and working `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 0,
        stdout: "2.0.0",
        stderr: "",
      }),
    );
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "fig");
    expect(version).toEqual("2.0.0");
  });

  it("missing `getVersionCommand` and not working `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 1,
        stdout: "",
        stderr: "No command available.",
      }),
    );
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "npm");
    expect(version).toBeUndefined();
  });

  it("missing `getVersionCommand` and throwing `command --version`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(Promise.reject());
    const newSpecData = { ...specData, getVersionCommand: undefined };
    const version = await getVersionFromFullFile(newSpecData, "npm");
    expect(version).toBeUndefined();
  });

  it("working `getVersionCommand`", async () => {
    vi.spyOn(wrappers, "executeCommand").mockReturnValue(
      Promise.resolve({
        status: 0,
        stdout: "",
        stderr: "No command available.",
      }),
    );
    const version = await getVersionFromFullFile(specData, "node");
    expect(version).toEqual("1.0.0");
  });
});
