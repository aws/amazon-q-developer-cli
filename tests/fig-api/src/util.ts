import { tmpdir } from "node:os";
import { mkdtempSync, realpathSync } from "node:fs";
import { join } from "node:path";

export const tempDir = realpathSync(mkdtempSync(join(tmpdir(), "fig-test-")));
