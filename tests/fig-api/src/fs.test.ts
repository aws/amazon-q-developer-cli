import { fs as figFs } from "@aws/amazon-q-developer-cli-api-bindings";
import nodeFs from "node:fs";
import { tempDir } from "./util.js";
import { describe, it, expect } from "vitest";

describe("fs api tests", () => {
  it("writes file", async () => {
    const file = `${tempDir}/write_file_test`;
    const content = "hello world";

    await figFs.write(file, content);

    expect(nodeFs.readFileSync(file).toString()).toBe(content);
  });

  it("list files", async () => {
    const nestedDir = `${tempDir}/list_files_test`;
    const files = ["a", "b", "c"].sort();

    nodeFs.mkdirSync(nestedDir);
    files.forEach((file) => nodeFs.writeFileSync(`${nestedDir}/${file}`, ""));

    const result = await figFs.list(nestedDir);
    expect(result.sort()).toEqual(files);
  });

  it("read file", async () => {
    const file = `${tempDir}/read_file_test`;
    const content = "hello world";

    nodeFs.writeFileSync(file, content);

    const result = await figFs.read(file);
    expect(result).toBe(content);
  });

  it("destination of symlink", async () => {
    const symlink = `${tempDir}/symlink_dest_test`;
    const target = `${tempDir}/symlink_dest_test_target`;

    nodeFs.writeFileSync(target, "hello world");
    nodeFs.symlinkSync(target, symlink);

    const result = await figFs.destinationOfSymbolicLink(symlink);
    expect(result).toBe(target);
  });

  it("create directory", async () => {
    const dir = `${tempDir}/create_dir_test`;

    await figFs.createDirectory(dir, false);

    expect(nodeFs.existsSync(dir)).toBe(true);
  });

  it("create directory recursively", async () => {
    const dir = `${tempDir}/create_dir_test_recursively/a/b/c`;

    await figFs.createDirectory(dir, true);

    expect(nodeFs.existsSync(dir)).toBe(true);
  });
});
