/**
 * Terminal notification support — ports the V1 Rust logic from
 * crates/chat-cli/src/cli/chat/util/mod.rs to TypeScript.
 */

export type NotificationMethod = 'bel' | 'osc9';

function detectNotificationMethod(): NotificationMethod | null {
  const termProgram = process.env.TERM_PROGRAM?.toLowerCase();
  if (
    termProgram === 'ghostty' ||
    termProgram === 'iterm.app' ||
    termProgram === 'wezterm' ||
    termProgram === 'windows_terminal'
  ) {
    return 'osc9';
  }

  const term = process.env.TERM;
  if (!term) return null;

  if (term.startsWith('xterm-ghostty')) return 'osc9';

  const belTerms = [
    'xterm',
    'xterm-256color',
    'screen',
    'screen-256color',
    'tmux',
    'tmux-256color',
    'rxvt',
    'rxvt-unicode',
    'linux',
    'konsole',
    'gnome',
    'gnome-256color',
    'alacritty',
    'iterm2',
    'eat-truecolor',
    'eat-256color',
    'eat-color',
  ];
  for (const t of belTerms) {
    if (term.startsWith(t)) return 'bel';
  }
  return null;
}

export function resolveNotificationMethod(
  setting?: string
): NotificationMethod | null {
  if (setting === 'bel') return 'bel';
  if (setting === 'osc9') return 'osc9';
  return detectNotificationMethod();
}

export function playNotification(
  method: NotificationMethod,
  message?: string
): void {
  if (method === 'bel') {
    process.stdout.write('\x07');
  } else {
    process.stdout.write(`\x1b]9;${message ?? 'Kiro CLI needs attention'}\x07`);
  }
}
