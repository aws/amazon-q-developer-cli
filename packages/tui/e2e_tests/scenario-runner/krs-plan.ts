import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import type { Scenario } from './types';

/**
 * Reads the KRS turns a scenario is served under the `krs-mock` backend.
 *
 * A scenario states that it is a krs-mock scenario by having a file here — there
 * is no field on the scenario itself, and nothing is inferred from its steps. The
 * scenario stays a description of behaviour; the model's side of the conversation
 * is written down, in full, next to it:
 *
 *     smoke/scenarios.json                    the scenario "tool-use-shell"
 *     smoke/fixtures/krs/tool-use-shell.json  the turns it is answered with
 *
 * Same convention as the acp-wire fixtures the `acp-mock` backend replays.
 */

const DEFAULT_KRS_FIXTURE_DIR = join(import.meta.dir, '../smoke/fixtures/krs');

export interface KrsPlan {
  scenarioId: string;
  /** Turns, in queue order, as `/__control/turns` accepts them. */
  turns: unknown[];
  /** The file they were read from, for error messages. */
  sidecarPath: string;
}

export interface ResolveKrsPlanOptions {
  /**
   * Directory holding the turn files, used as given. Not derived from the
   * runner's `--fixtures-dir`: that flag points at the acp-wire directory, which
   * is a sibling of this one, not its parent.
   */
  krsDir?: string;
}

/**
 * Loads a scenario's turns.
 *
 * Throws when there is no file for it: running a scenario against the fake KRS
 * with nothing scripted would fail on the first call anyway, several steps from
 * the cause.
 */
export function resolveKrsPlan(
  scenario: Scenario,
  options: ResolveKrsPlanOptions = {}
): KrsPlan {
  const sidecarPath = join(options.krsDir ?? DEFAULT_KRS_FIXTURE_DIR, `${scenario.id}.json`);

  if (!existsSync(sidecarPath)) {
    throw new Error(
      `scenario "${scenario.id}" has no KRS turns. Write them to ${sidecarPath} ` +
        `(schema: smoke/krs-turns.schema.json), or run it under a different backend.`
    );
  }

  return {
    scenarioId: scenario.id,
    turns: readTurns(sidecarPath),
    sidecarPath,
  };
}

function readTurns(path: string): unknown[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`KRS turns ${path} are not valid JSON`, { cause: error });
  }

  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(`KRS turns ${path} must be an object with a "turns" array`);
  }
  const { turns } = parsed as { turns?: unknown };
  if (!Array.isArray(turns)) {
    throw new Error(`KRS turns ${path} must be an object with a "turns" array`);
  }
  if (turns.length === 0) {
    // An empty file would leave every call unanswered.
    throw new Error(`KRS turns ${path} declare no turns`);
  }
  return turns;
}
