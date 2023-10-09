/* eslint-disable import/no-extraneous-dependencies */
import {
  Project,
  PropertySignature,
  SourceFile,
  CodeBlockWriter,
  IndentationText,
} from "ts-morph";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/* eslint-enable import/no-extraneous-dependencies */
const capitalizeFirstLetter = (str: string) =>
  str.charAt(0).toUpperCase() + str.slice(1);

const lowercaseFirstLetter = (str: string) =>
  str.charAt(0).toLowerCase() + str.slice(1);

const normalize = (type: string): string => {
  let normalized = type;
  if (type.endsWith("Request")) {
    normalized = type.replace("Request", "");
  } else if (type.endsWith("Response")) {
    normalized = type.replace("Response", "");
  }

  return capitalizeFirstLetter(normalized);
};

const getSubmessageTypes = (bindings: SourceFile, interfaceName: string) => {
  const interfaceRef = bindings.getInterface(interfaceName)!;
  const submessage = interfaceRef.getProperties()[1];

  const submessageUnion = submessage
    .getChildren()
    .filter((elm) => elm.getKindName() === "UnionType")[0];
  const literals = submessageUnion
    .getChildren()[0]
    .getChildren()
    .filter((elm) => elm.getKindName() === "TypeLiteral");
  const types = literals
    .map((elm) => elm.getChildren()[1])
    .map((elm) => elm.getChildren()[1]);
  return types.map((prop) => (prop as PropertySignature).getName());
};

const writeGenericSendRequestWithResponseFunction = (
  writer: CodeBlockWriter,
  endpoint: string
) => {
  const lowercasedEndpoint = lowercaseFirstLetter(endpoint);

  const template = `export async function send${endpoint}Request(
request: ${endpoint}Request
): Promise<${endpoint}Response> {
  return new Promise((resolve, reject) => {
    sendMessage(
      { $case: "${lowercasedEndpoint}Request", ${lowercasedEndpoint}Request: request },
      (response) => {
        switch (response?.$case) {
          case "${lowercasedEndpoint}Response":
            resolve(response.${lowercasedEndpoint}Response);
            break;
          case "error":
            reject(Error(response.error));
            break;
          default:
            reject(
              Error(
                  \`Invalid response '\${response?.$case}' for '${endpoint}Request'\`
              )
            );
        }
      }
    );
  });
}`;

  writer.writeLine(template).blankLine();
};

const writeGenericSendRequestFunction = (
  writer: CodeBlockWriter,
  endpoint: string
) => {
  const lowercasedEndpoint = lowercaseFirstLetter(endpoint);

  const template = `export async function send${endpoint}Request(
  request: ${endpoint}Request
): Promise<void> {
  return new Promise((resolve, reject) => {
    sendMessage(
      { $case: "${lowercasedEndpoint}Request", ${lowercasedEndpoint}Request: request },
      (response) => {
        switch (response?.$case) {
          case "success":
            resolve();
            break;
          case "error":
            reject(Error(response.error));
            break;
          default:
            reject(
              Error(
                \`Invalid response '\${response?.$case}' for '${endpoint}Request'\`
              )
            );
        }
      }
    );
  });
}`;
  writer.writeLine(template).blankLine();
};

const project = new Project({
  manipulationSettings: {
    indentationText: IndentationText.TwoSpaces,
  },
});

project.addSourceFilesAtPaths(join(__dirname, "../src/*.ts"));

const text = readFileSync(
  "node_modules/@fig/fig-api-proto/dist/fig.pb.ts",
  "utf8"
);
const protobufBindings = project.createSourceFile("fig.pb.ts", text);

const requestTypes = getSubmessageTypes(
  protobufBindings,
  "ClientOriginatedMessage"
);
const responseTypes = getSubmessageTypes(
  protobufBindings,
  "ServerOriginatedMessage"
).filter((type) => type.includes("Response"));

const [requestsWithMatchingResponses, otherRequests] = requestTypes
  .filter((request) => request !== "notificationRequest")
  .reduce(
    (result, request) => {
      const [matchingResponse, other] = result;

      const endpoint = lowercaseFirstLetter(normalize(request));

      if (responseTypes.indexOf(`${endpoint}Response`) !== -1) {
        return [matchingResponse.concat([request]), other];
      }
      return [matchingResponse, other.concat([request])];
    },
    [[] as string[], [] as string[]]
  );

console.log(requestsWithMatchingResponses, otherRequests);

const protoVersion = JSON.parse(
  readFileSync(join(__dirname, "../../../proto/package.json"), "utf8")
).version;

const sourceFile = project.createSourceFile(
  join(__dirname, "../src/requests.ts"),
  (writer) => {
    writer.writeLine(
      `/* Autogenerated by generate-requests.ts for proto v${protoVersion}`
    );
    writer.writeLine(
      ` * Do not edit directly! Instead run 'npm run generate-requests' in typescript-api-bindings`
    );
    writer.writeLine(` */`).blankLine();
    writer.writeLine(`/* eslint-disable max-len */`).blankLine();

    const responses = requestsWithMatchingResponses.map((request) =>
      request.replace("Request", "Response")
    );
    const imports = requestsWithMatchingResponses
      .concat(responses)
      .concat(otherRequests)
      .sort()
      .map(capitalizeFirstLetter);
    writer.writeLine(
      `import { \n${imports.join(
        ",\n"
      )}\n } from "@fig/fig-api-proto/dist/fig.pb";`
    );
    writer.writeLine(`import { sendMessage } from "./core";`).blankLine();

    requestsWithMatchingResponses.forEach((request) =>
      writeGenericSendRequestWithResponseFunction(writer, normalize(request))
    );
    otherRequests.forEach((request) =>
      writeGenericSendRequestFunction(writer, normalize(request))
    );
  },
  { overwrite: true }
);

sourceFile.formatText();
sourceFile.saveSync();