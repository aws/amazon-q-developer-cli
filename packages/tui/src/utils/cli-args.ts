/**
 * CLI argument parsing for the TUI.
 *
 * Parses process.argv and builds the extra args forwarded to the ACP backend.
 * The ACP args contract is defined by the generated {@link AcpSpawnArgs} type
 * (Rust → typeshare → TypeScript) so both sides stay in sync.
 *
 * To add a new flag, add an entry to {@link FLAG_DEFS} — parsing, forwarding,
 * and `--key=value` handling are all automatic.
 */

import type { AcpSpawnArgs } from '../types/generated/agent.js';

export type { AcpSpawnArgs };

/**
 * Parsed CLI arguments.
 *
 * ACP-forwarded fields come from the generated {@link AcpSpawnArgs}.
 * TUI-only fields (noInteractive, input, resume, resumePicker) are local additions.
 */
export interface CliArgs extends AcpSpawnArgs {
  /** Run without expecting user input (--no-interactive / --non-interactive). TUI-only. */
  noInteractive: boolean;
  /** The first question to ask (positional arg). TUI-only. */
  input?: string;
  /** Resume the most recent conversation from this directory (--resume / -r). TUI-only. */
  resume: boolean;
  /** Resume a specific conversation by session ID (--resume-id <id>). TUI-only. */
  resumeId?: string;
  /** Interactively select a conversation to resume (--resume-picker). TUI-only. */
  resumePicker: boolean;
  /** Stress test mode: send expensive prompts in a loop (--stress). TUI-only. */
  stress: boolean;
  /** Number of stress iterations (--stress-iterations N, default 100). TUI-only. */
  stressIterations?: number;
}

// ── Flag definitions ────────────────────────────────────────────────────
// Each entry declares one CLI flag. The parser and buildAcpArgs both
// derive their behaviour from this single table.

type StringKeys = {
  [K in keyof CliArgs]-?: NonNullable<CliArgs[K]> extends string ? K : never;
}[keyof CliArgs];
type BooleanKeys = {
  [K in keyof CliArgs]-?: NonNullable<CliArgs[K]> extends boolean ? K : never;
}[keyof CliArgs];
type StringListKeys = {
  [K in keyof CliArgs]-?: NonNullable<CliArgs[K]> extends string[] ? K : never;
}[keyof CliArgs];

type FlagDef =
  | { type: 'string'; key: StringKeys; flags: string[]; acp?: string }
  | { type: 'boolean'; key: BooleanKeys; flags: string[]; acp?: string }
  | { type: 'string-list'; key: StringListKeys; flags: string[]; acp?: string }
  | { type: 'skip'; flags: string[]; hasValue?: boolean };

const FLAG_DEFS: FlagDef[] = [
  {
    type: 'string',
    key: 'agent',
    flags: ['--agent', '--profile'],
    acp: '--agent',
  },
  { type: 'string', key: 'model', flags: ['--model'], acp: '--model' },
  { type: 'string', key: 'resumeId', flags: ['--resume-id'] },
  {
    type: 'boolean',
    key: 'trustAllTools',
    flags: ['--trust-all-tools', '-a'],
    acp: '--trust-all-tools',
  },
  {
    type: 'string-list',
    key: 'trustTools',
    flags: ['--trust-tools'],
    acp: '--trust-tools',
  },
  {
    type: 'boolean',
    key: 'noInteractive',
    flags: ['--no-interactive', '--non-interactive'],
  },
  { type: 'boolean', key: 'resume', flags: ['--resume', '-r'] },
  { type: 'boolean', key: 'resumePicker', flags: ['--resume-picker'] },
  { type: 'boolean', key: 'stress', flags: ['--stress'] },
  { type: 'string', key: 'stressIterations' as any, flags: ['--stress-iterations'] },
  // consumed by Rust ChatArgs before TUI is launched — skip without error
  { type: 'skip', flags: ['--tui'] },
];

// Build a lookup map: flag string → FlagDef (built once at module load)
const FLAG_MAP = new Map<string, FlagDef>();
for (const def of FLAG_DEFS) {
  for (const f of def.flags) {
    FLAG_MAP.set(f, def);
  }
}

/**
 * Parse CLI arguments from process.argv.
 *
 * Supports both `--key value` and `--key=value` syntax for all flags.
 */
export function parseCliArgs(): CliArgs {
  const args = process.argv.slice(2);
  const result: CliArgs = {
    trustAllTools: false,
    noInteractive: false,
    resume: false,
    resumePicker: false,
    stress: false,
  };

  // Skip past "chat" subcommand if present
  let startIdx = 0;
  if (args[0] === 'chat') {
    startIdx = 1;
  }

  for (let i = startIdx; i < args.length; i++) {
    const raw = args[i]!;

    // Normalize --key=value → key + value
    const eqIdx = raw.indexOf('=');
    const flag = eqIdx > 0 ? raw.slice(0, eqIdx) : raw;
    const eqValue = eqIdx > 0 ? raw.slice(eqIdx + 1) : undefined;

    const def = FLAG_MAP.get(flag);

    if (def) {
      if (def.type === 'string') {
        (result as any)[def.key] = eqValue ?? args[++i];
      } else if (def.type === 'string-list') {
        const csv = eqValue ?? args[++i] ?? '';
        (result as any)[def.key] = csv.split(',');
      } else if (def.type === 'boolean') {
        (result as any)[def.key] = true;
      } else if (def.type === 'skip' && def.hasValue) {
        if (eqValue === undefined) i++; // consume next arg
      }
    } else if (raw.startsWith('-')) {
      // Unknown flag — skip its value if the next arg looks like a value
      if (eqValue === undefined) {
        const next = args[i + 1];
        if (next && !next.startsWith('-')) i++;
      }
    } else if (!result.input) {
      result.input = raw;
    }
  }

  return result;
}

/**
 * Build the extra arguments to forward to the ACP backend process.
 *
 * Only flags with an `acp` field in {@link FLAG_DEFS} are forwarded. TUI-only
 * fields like `noInteractive` are intentionally excluded.
 */
export function buildAcpArgs(cliArgs: Partial<AcpSpawnArgs>): string[] {
  const args: string[] = [];

  for (const def of FLAG_DEFS) {
    if (def.type === 'skip' || !def.acp) continue;
    const value = (cliArgs as any)[def.key];
    if (def.type === 'string' && value) {
      args.push(def.acp, value);
    } else if (
      def.type === 'string-list' &&
      Array.isArray(value) &&
      value.length > 0
    ) {
      args.push(def.acp, value.join(','));
    } else if (def.type === 'boolean' && value) {
      args.push(def.acp);
    }
  }

  return args;
}
