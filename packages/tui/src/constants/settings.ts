/**
 * Setting keys matching the dotted names in the Rust settings store.
 * Source of truth: crates/chat-cli-v2/src/database/settings.rs
 *
 * TODO: update when chat-cli and chat-cli-v2 settings are merged eventually
 */
export const Settings = {
  CHAT_GREETING_ENABLED: 'chat.greeting.enabled',
  CHAT_ENABLE_NOTIFICATIONS: 'chat.enableNotifications',
  CHAT_NOTIFICATION_METHOD: 'chat.notificationMethod',
  CHAT_AUTO_EXPAND_TOOL_OUTPUT: 'chat.autoExpandToolOutput',
  CHAT_DISABLE_WRAP: 'chat.disableWrap',
  CHAT_DISABLE_TRUST_ALL_CONFIRMATION: 'chat.disableTrustAllConfirmation',
  CHAT_KEYBINDINGS_CANCEL_STREAM: 'chat.keybindings.cancelStream',
  CHAT_KEYBINDINGS_CLOSE_MENU: 'chat.keybindings.closeMenu',
  CHAT_KEYBINDINGS_QUIT: 'chat.keybindings.quit',
  CHAT_UI_MODE: 'chat.ui.mode',
  CHAT_ASCII_MODE: 'chat.allowAsciiArt',
  CHAT_ANIMATIONS: 'chat.allowAnimations',
  CHAT_ICONS: 'chat.allowIcons',
  CHAT_SHOW_THINKING: 'chat.showThinking',
  CHAT_HISTORY_MODE: 'chat.historyMode',
  // Verbosity-config mirror keys. The lite UI's /verbosity menu is the
  // primary write surface for these; the modern TUI consumes them via
  // getVerboseDisplay() so the same toggle takes effect in both modes.
  // Lite's setVerboseConfig() writes both lite_verbose.json (legacy
  // mirror) and cli.json (canonical), and getVerboseDisplay() resolves
  // each field with cli.json > lite_verbose.json > DEFAULT_DISPLAY.
  // See packages/tui/src/lite/verbose.ts for the field-by-field
  // semantics.
  CHAT_TOOLS_FILTERS: 'chat.tools.filters',
  CHAT_TOOLS_SHOW_REASONING: 'chat.tools.showReasoning',
  CHAT_TOOLS_ARGS_MODE: 'chat.tools.argsMode',
  CHAT_TOOLS_SHOW_ELAPSED: 'chat.tools.showElapsed',
  CHAT_TOOLS_ARGS_MAX_LINES: 'chat.tools.argsMaxLines',
  CHAT_TOOLS_OUTPUT_MAX_LINES: 'chat.tools.outputMaxLines',
  CHAT_TOOLS_ARGS_MAX_CHARS: 'chat.tools.argsMaxChars',
  CHAT_TOOLS_OUTPUT_MAX_CHARS: 'chat.tools.outputMaxChars',
  CHAT_TOOLS_SHOW_WRITE_DIFFS: 'chat.tools.showWriteDiffs',
  CHAT_SHOW_TASKS: 'chat.showTasks',
  CHAT_SUBAGENT_SHOW_PIPELINE: 'chat.subagent.showPipeline',
  CHAT_SUBAGENT_SHOW_PROMPTS: 'chat.subagent.showPrompts',
  CHAT_SUBAGENT_SHOW_ROLES: 'chat.subagent.showRoles',
  CHAT_SUBAGENT_SHOW_DEPS: 'chat.subagent.showDeps',
  CHAT_SUBAGENT_SHOW_RESPONSES: 'chat.subagent.showResponses',
  CHAT_TERMINAL_TITLE: 'chat.terminalTitle',
  CHAT_DEFAULT_INTERRUPT_BEHAVIOR: 'chat.defaultInterruptBehavior',
  CHAT_KEYBINDINGS_TOGGLE_INTERRUPT_BEHAVIOR:
    'chat.keybindings.toggleInterruptBehavior',
} as const;
