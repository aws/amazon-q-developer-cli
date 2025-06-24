import { existsSync as exists } from "node:fs";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";

import { Task, TaskSet } from "./tasks.js";

if (process.env.CI === undefined && !exists("./types/.glib.d.ts")) {
  console.log("Generating types, this will take a moment...");

  try {
    await fs.mkdir("./types");
  } catch {
    // ignore
  }

  // Generate a bunch of typescript headers using this handy tool!

  await new Promise((resolve, reject) => {
    const child_process = spawn("yarn", [
      "gi-ts",
      "generate",
      "--all",
      "--out=types",
      "--propertyCase=underscore",
      "--withDocs",
    ]);

    child_process.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(
          new Error(
            `'yarn gi-ts generate --all --out=types --propertyC=underscore --withDocs' failed with exit code ${code}.`,
          ),
        );
      }
    });
  });

  // For whatever gosh darn reason, this gi-ts tool outputs:
  // ```ts
  // import * as GObject from "gobject";
  // ```
  //
  // ...when gjs only supports either this:
  // ```js
  // const { GObject } = imports.gi;
  // ```
  // ...or this:
  // ```js
  // import * as GObject from "gi://GObject";
  // ```
  //
  // so... we have to replace all of those with relative imports (IE "./glib"
  // instead of "glib") and then create an index.d.ts file that defines the
  // global imports object.

  const regexp =
    /import \* as ([A-Za-z_][A-Za-z_0-9]*) from "([A-Za-z_][A-Za-z_0-9]*)";/g;

  // List of spawned tasks -- will be waited on with Promise.all later.
  const tasks = new TaskSet();

  for await (const entry of await fs.opendir("./types")) {
    if (entry.isFile()) {
      if (entry.name === "gi.d.ts") continue;
      if (entry.name === "imports.d.ts") continue;
      if (entry.name === "index.d.ts") continue;

      tasks.add(
        new Task(async () => {
          const content = (
            await fs.readFile(path.join("./types", entry.name), "utf8")
          ).replaceAll(regexp, (_, proper_name, improper_name) => {
            return `import * as ${proper_name} from "./.${improper_name}";`;
          });
          await fs.writeFile(
            path.join("./types", `.${entry.name}`),
            content,
            "utf8",
          );
          await fs.rm(path.join("./types", entry.name));
        }),
      );
    }
  }

  await tasks.wait();

  console.log("Generated types!");
}
