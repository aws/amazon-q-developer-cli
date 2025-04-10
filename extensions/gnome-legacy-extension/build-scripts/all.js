import { chdir } from "node:process";
import { exec } from "node:child_process";
import { promisify } from "node:util";
import { Worker } from "node:worker_threads";

chdir((await promisify(exec)("git rev-parse --show-toplevel")).stdout.trim());

await Promise.all([
  new Promise((resolve, reject) => {
    const worker = new Worker("./build/extension.js");
    worker.on("message", console.log);
    worker.on("exit", resolve);
    worker.on("error", reject);
  }),
  new Promise((resolve, reject) => {
    const worker = new Worker("./build/types.js");
    worker.on("message", console.log);
    worker.on("exit", resolve);
    worker.on("error", reject);
  }),
]);
