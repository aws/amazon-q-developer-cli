import { fs as FileSystem } from "@aws/amazon-q-developer-cli-api-bindings";
export const fread = (path) => FileSystem.read(path).then((out) => out ?? "");
//# sourceMappingURL=fs.js.map