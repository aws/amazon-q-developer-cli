import { create } from "@bufbuild/protobuf";
import { sendWriteFileRequest, sendReadFileRequest, sendDestinationOfSymbolicLinkRequest, sendContentsOfDirectoryRequest, sendAppendToFileRequest, sendCreateDirectoryRequest, } from "./requests.js";
import { FilePathSchema, } from "@aws/amazon-q-developer-cli-proto/fig";
function filePath(options) {
    return create(FilePathSchema, options);
}
export async function write(path, contents) {
    return sendWriteFileRequest({
        path: filePath({ path, expandTildeInPath: true }),
        data: { case: "text", value: contents },
    });
}
export async function append(path, contents) {
    return sendAppendToFileRequest({
        path: filePath({ path, expandTildeInPath: true }),
        data: { case: "text", value: contents },
    });
}
export async function read(path) {
    const response = await sendReadFileRequest({
        path: filePath({ path, expandTildeInPath: true }),
    });
    if (response.type?.case === "text") {
        return response.type.value;
    }
    return null;
}
export async function list(path) {
    const response = await sendContentsOfDirectoryRequest({
        directory: filePath({ path, expandTildeInPath: true }),
    });
    return response.fileNames;
}
export async function destinationOfSymbolicLink(path) {
    const response = await sendDestinationOfSymbolicLinkRequest({
        path: filePath({ path, expandTildeInPath: true }),
    });
    return response.destination?.path;
}
export async function createDirectory(path, recursive) {
    return sendCreateDirectoryRequest({
        path: filePath({ path, expandTildeInPath: true }),
        recursive,
    });
}
//# sourceMappingURL=filesystem.js.map