import os from "node:os";
import fs from "node:fs";
import { describe, expect, it } from "vitest";

describe("constants", () => {
  it("os", () => {
    if (os.platform() === "win32") {
      expect(window.fig.constants?.os).toBe("windows");
    } else if (os.platform() === "darwin") {
      expect(window.fig.constants?.os).toBe("macos");
    } else if (os.platform() === "linux") {
      expect(window.fig.constants?.os).toBe("linux");
    }
  });

  it("arch", () => {
    if (os.arch() === "x64") {
      expect(window.fig.constants?.arch).toBe("x86_64");
    } else if (os.arch() === "arm64") {
      expect(window.fig.constants?.arch).toBe("aarch64");
    }
  });

  it("user", () => {
    expect(window.fig.constants?.user).toBe(os.userInfo().username);
    expect(window.fig.constants?.home).toBe(os.homedir());
  });

  it("env", () => {
    expect(window.fig.constants?.env).toHaveProperty("PATH");
  });

  it("version", () => {
    const workspaceToml = fs.readFileSync(
      __dirname + "/../../../Cargo.toml",
      "utf8",
    );
    const version = workspaceToml.match(/version = "([^"]+)"/)?.[1];
    expect(version).toBeTruthy();
    expect(window.fig.constants?.version).toBe(version);
  });
});
