import { exec } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { env } from "node:process";
import { promisify } from "node:util";

import { Task, TaskSet } from "./tasks.js";
import { File, Folder, Zip } from "./zip.js";

if (process.platform !== "linux") {
  console.log(
    `Extension building only supported on Linux. Current platform: ${process.platform}`,
  );
  process.exit(0);
}

console.log("Bundling extension...");

const tasks = new TaskSet();
const zip = new Zip();

tasks.add(
  new Task(async () => {
    const metadata = await fs.readFile("./src/metadata.json", "utf8");
    zip.add("metadata.json", new File(metadata));
  }),
);

tasks.add(
  new Task(async () => {
    const resources = new Folder();

    await promisify(exec)(
      "cd './resources' && glib-compile-resources --target './resources.gresource' './resources.gresource.xml'",
    );

    resources.add(
      "amazon-q-for-cli-gnome-integration.gresource",
      new File(await fs.readFile("./resources/resources.gresource")),
    );

    zip.add("resources", resources);
  }),
);

tasks.add(
  new Task(async () => {
    const schemas = new Folder();

    try {
      await fs.rm("./schemas/gschemas.compiled");
    } catch {
      // ignore
    }

    await promisify(exec)("glib-compile-schemas schemas");

    schemas.add(
      "gschemas.compiled",
      new File(await fs.readFile("./schemas/gschemas.compiled")),
    );

    schemas.add(
      "org.gnome.shell.extensions.amazon-q-for-cli-gnome-integration.gschema.xml",
      new File(
        await fs.readFile(
          "./schemas/org.gnome.shell.extensions.amazon-q-for-cli-gnome-integration.gschema.xml",
        ),
      ),
    );

    zip.add("schemas", schemas);
  }),
);

for await (const entry of await fs.opendir("./dist")) {
  if (entry.isFile()) {
    if (!entry.name.endsWith(".js")) continue;

    tasks.add(
      new Task(async () => {
        switch (env.RELEASE) {
          case "0":
          case "false":
          case undefined:
            {
              const source = await (async () => {
                let source = await fs.readFile(
                  path.join("./dist", entry.name),
                  "utf8",
                );

                return source;
              })();

              const bundled = (() => {
                let bundled = "const DEBUG=true;\n";
                bundled += source;
                // bundled = bundled.replaceAll(
                //   /this.#([a-zA-Z_$][a-zA-Z0-9_$]*)/g,
                //   (_, ident) => `this.$${ident}`,
                // );
                // bundled = bundled.replaceAll(
                //   /#([a-zA-Z_$][a-zA-Z0-9_$]*?)\(/g,
                //   (_, ident) => `$${ident}(`,
                // );
                return bundled;
              })();

              zip.add(entry.name, new File(bundled));
            }
            break;
          default:
            {
              const source = await (async () => {
                let source = await fs.readFile(
                  path.join("./dist", entry.name),
                  "utf8",
                );

                return source;
              })();

              const bundled = (() => {
                let bundled = "const DEBUG=false;\n";
                bundled += source;
                // bundled = bundled.replaceAll(
                //   /this.#([a-zA-Z_$][a-zA-Z0-9_$]*)/g,
                //   (_, ident) => `this.$${ident}`,
                // );
                // bundled = bundled.replaceAll(
                //   /#([a-zA-Z_$][a-zA-Z0-9_$]*?)\(/g,
                //   (_, ident) => `$${ident}(`,
                // );
                return bundled;
              })();

              zip.add(entry.name, new File(bundled));
            }
            break;
        }
      }),
    );
  }
}

await tasks.wait();

await zip.save("./amazon-q-for-cli-gnome-integration@aws.amazon.com.zip");

console.log("Extension bundled!");
