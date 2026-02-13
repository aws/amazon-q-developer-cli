import { parseArgs } from 'util';

/**
 * CLI arguments that the TUI needs to forward to the ACP backend.
 */
export interface CliArgs {
  /** Agent name to use (--agent) */
  agent?: string;
  /** Auto-approve all tool permission requests (--trust-all-tools / -a) */
  trustAllTools?: boolean;
  /** Run without expecting user input (--no-interactive / --non-interactive) */
  noInteractive?: boolean;
  /** The first question to ask (positional arg) */
  input?: string;
}

/**
 * Parse CLI arguments passed to the TUI process.
 *
 * The Rust launcher forwards all original args (e.g. ["chat", "--agent", "foo"])
 * so we skip the "chat" subcommand and extract the flags we care about.
 */
export function parseCliArgs(): CliArgs {
  try {
    const { values, positionals } = parseArgs({
      // Skip bun binary + script path; the rest is ["chat", "--agent", ...]
      args: process.argv.slice(2),
      options: {
        agent: { type: 'string', alias: 'profile' },
        // Declare other flags so parseArgs doesn't throw on unknown args
        resume: { type: 'boolean', short: 'r' },
        'resume-picker': { type: 'boolean' },
        model: { type: 'string' },
        'trust-all-tools': { type: 'boolean', short: 'a' },
        'trust-tools': { type: 'string' },
        'no-interactive': { type: 'boolean' },
        'non-interactive': { type: 'boolean' },
        'list-sessions': { type: 'boolean', short: 'l' },
        'delete-session': { type: 'string', short: 'd' },
        'legacy-mode': { type: 'boolean' },
        wrap: { type: 'string', short: 'w' },
      },
      strict: false,
      allowPositionals: true,
    });

    // Positionals: ["chat", "Hello world"] — skip "chat" subcommand
    const inputPositional = positionals
      .filter((p) => p !== 'chat')
      .join(' ')
      .trim();

    return {
      agent: values.agent as string | undefined,
      trustAllTools: values['trust-all-tools'] as boolean | undefined,
      noInteractive: (values['no-interactive'] || values['non-interactive']) as
        | boolean
        | undefined,
      input: inputPositional || undefined,
    };
  } catch {
    // If parsing fails, return empty — don't crash the TUI
    return {};
  }
}

/**
 * Build extra args to append to the ACP spawn command based on parsed CLI args.
 */
export function buildAcpArgs(cliArgs: CliArgs): string[] {
  const args: string[] = [];
  if (cliArgs.agent) {
    args.push('--agent', cliArgs.agent);
  }
  if (cliArgs.trustAllTools) {
    args.push('--trust-all-tools');
  }
  if (cliArgs.noInteractive) {
    args.push('--no-interactive');
  }
  return args;
}
