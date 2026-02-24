/**
 * CLI argument parsing for the TUI.
 *
 * Parses process.argv and builds the extra args forwarded to the ACP backend.
 * The ACP args contract is defined by the generated {@link AcpSpawnArgs} type
 * (Rust → typeshare → TypeScript) so both sides stay in sync.
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
  /** Interactively select a conversation to resume (--resume-picker). TUI-only. */
  resumePicker: boolean;
}

/**
 * Parse CLI arguments from process.argv.
 *
 * Supports:
 *   --agent <name> / --profile <name>
 *   --model <id>
 *   --trust-all-tools / -a
 *   --no-interactive / --non-interactive
 *   positional input (first non-flag argument after "chat")
 */
export function parseCliArgs(): CliArgs {
  const args = process.argv.slice(2);
  const result: CliArgs = {
    trustAllTools: false,
    noInteractive: false,
    resume: false,
    resumePicker: false,
  };

  // Skip past "chat" subcommand if present
  let startIdx = 0;
  if (args[0] === 'chat') {
    startIdx = 1;
  }

  for (let i = startIdx; i < args.length; i++) {
    const arg = args[i]!;

    if (arg === '--agent' || arg === '--profile') {
      result.agent = args[++i];
    } else if (arg === '--model') {
      result.model = args[++i];
    } else if (arg === '--trust-all-tools' || arg === '-a') {
      result.trustAllTools = true;
    } else if (arg === '--no-interactive' || arg === '--non-interactive') {
      result.noInteractive = true;
    } else if (arg === '--resume' || arg === '-r') {
      result.resume = true;
    } else if (arg === '--resume-picker') {
      result.resumePicker = true;
    } else if (arg.startsWith('-')) {
      // Unknown flag — skip its value if the next arg looks like a value (not another flag)
      const next = args[i + 1];
      if (next && !next.startsWith('-')) {
        i++;
      }
    } else if (!result.input) {
      // First positional arg is the input
      result.input = arg;
    }
  }

  return result;
}

/**
 * Build the extra arguments to forward to the ACP backend process.
 *
 * Only flags matching {@link AcpSpawnArgs} are forwarded — TUI-only
 * fields like `noInteractive` are intentionally excluded.
 */
export function buildAcpArgs(cliArgs: AcpSpawnArgs): string[] {
  const args: string[] = [];

  if (cliArgs.agent) {
    args.push('--agent', cliArgs.agent);
  }
  if (cliArgs.model) {
    args.push('--model', cliArgs.model);
  }
  if (cliArgs.trustAllTools) {
    args.push('--trust-all-tools');
  }

  return args;
}
