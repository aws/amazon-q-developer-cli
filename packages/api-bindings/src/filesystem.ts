import { create } from "@bufbuild/protobuf";
import {
  sendWriteFileRequest,
  sendReadFileRequest,
  sendDestinationOfSymbolicLinkRequest,
  sendContentsOfDirectoryRequest,
  sendAppendToFileRequest,
  sendCreateDirectoryRequest,
} from "./requests.js";
import {
  type FilePath,
  FilePathSchema,
} from "@aws/amazon-q-developer-cli-proto/fig";

function filePath(options: Omit<FilePath, "$typeName">) {
  return create(FilePathSchema, options);
}

export async function write(path: string, contents: string) {
  return sendWriteFileRequest({
    path: filePath({ path, expandTildeInPath: true }),
    data: { case: "text", value: contents },
  });
}

export async function append(path: string, contents: string) {
  return sendAppendToFileRequest({
    path: filePath({ path, expandTildeInPath: true }),
    data: { case: "text", value: contents },
  });
}

export async function read(path: string) {
  const response = await sendReadFileRequest({
    path: filePath({ path, expandTildeInPath: true }),
  });
  if (response.type?.case === "text") {
    return response.type.value;
  }
  return null;
}

export async function list(path: string) {
  const response = await sendContentsOfDirectoryRequest({
    directory: filePath({ path, expandTildeInPath: true }),
  });
  return response.fileNames;
}

export async function destinationOfSymbolicLink(path: string) {
  const response = await sendDestinationOfSymbolicLinkRequest({
    path: filePath({ path, expandTildeInPath: true }),
  });
  return response.destination?.path;
}

export async function createDirectory(path: string, recursive: boolean) {
  return sendCreateDirectoryRequest({
    path: filePath({ path, expandTildeInPath: true }),
    recursive,
  });
}
