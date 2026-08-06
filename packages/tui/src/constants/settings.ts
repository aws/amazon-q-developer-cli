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
  CHAT_PRESERVE_SCROLLBACK: 'chat.preserveScrollback',
  CHAT_DISABLE_TRUST_ALL_CONFIRMATION: 'chat.disableTrustAllConfirmation',
  CHAT_KEYBINDINGS_CANCEL_STREAM: 'chat.keybindings.cancelStream',
  CHAT_KEYBINDINGS_CLOSE_MENU: 'chat.keybindings.closeMenu',
  CHAT_KEYBINDINGS_QUIT: 'chat.keybindings.quit',
  CHAT_UI_MODE: 'chat.ui.mode',
  CHAT_ASCII_MODE: 'chat.allowAsciiArt',
  CHAT_ANIMATIONS: 'chat.allowAnimations',
  CHAT_ICONS: 'chat.allowIcons',
  CHAT_SHOW_THINKING: 'chat.showThinking',
  CHAT_SHOW_THINKING_TIPS: 'chat.showThinkingTips',
  CHAT_HISTORY_MODE: 'chat.historyMode',
  // Independent verbosity records stored in the existing global cli.json.
  CHAT_VERBOSITY_LITE: 'chat.verbosity.lite',
  CHAT_VERBOSITY_TUI: 'chat.verbosity.tui',
  // Legacy shared verbosity keys. Read only for migration by verbose.ts;
  // new writes use the surface records above.
  CHAT_TOOLS_FILTERS: 'chat.tools.filters',
  CHAT_TOOLS_SHOW_REASONING: 'chat.tools.showReasoning',
  CHAT_TOOLS_ARGS_MODE: 'chat.tools.argsMode',
  CHAT_TOOLS_SHOW_ELAPSED: 'chat.tools.showElapsed',
  CHAT_TOOLS_ARGS_MAX_LINES: 'chat.tools.argsMaxLines',
  CHAT_TOOLS_OUTPUT_MAX_LINES: 'chat.tools.outputMaxLines',
  CHAT_TOOLS_ARGS_MAX_CHARS: 'chat.tools.argsMaxChars',
  CHAT_TOOLS_OUTPUT_MAX_CHARS: 'chat.tools.outputMaxChars',
  // Whether a finished tool call keeps its output body in scrollback after the
  // turn ends (true) or collapses to a one-line summary on the next message
  // (false). Consumed by the modern TUI's static-turn render.
  CHAT_TOOLS_PERSIST_OUTPUT: 'chat.tools.persistOutput',
  CHAT_TOOLS_SHOW_WRITE_DIFFS: 'chat.tools.showWriteDiffs',
  CHAT_SHOW_TASKS: 'chat.showTasks',
  CHAT_SUBAGENT_SHOW_PIPELINE: 'chat.subagent.showPipeline',
  CHAT_SUBAGENT_SHOW_PROMPTS: 'chat.subagent.showPrompts',
  CHAT_SUBAGENT_SHOW_ROLES: 'chat.subagent.showRoles',
  CHAT_SUBAGENT_SHOW_DEPS: 'chat.subagent.showDeps',
  CHAT_SUBAGENT_SHOW_RESPONSES: 'chat.subagent.showResponses',
  CHAT_TERMINAL_TITLE: 'chat.terminalTitle',
  // Per-surface `{ segmentId: boolean }` overrides for status-line visibility,
  // holding only the ids that differ from the default. Read and written by the
  // TUI alone.
  CHAT_STATUS_LINE_TUI: 'chat.statusLine.tui',
  CHAT_STATUS_LINE_LITE: 'chat.statusLine.lite',
  CHAT_DEFAULT_INTERRUPT_BEHAVIOR: 'chat.defaultInterruptBehavior',
  CHAT_KEYBINDINGS_TOGGLE_INTERRUPT_BEHAVIOR:
    'chat.keybindings.toggleInterruptBehavior',
  // Sticky-default storage keys (shared with v2). `chat.defaultModel` holds the
  // single global default model id; `chat.modelDefaults` holds the per-model
  // nested defaults object (effort lives under each model's family schema path).
  CHAT_DEFAULT_MODEL: 'chat.defaultModel',
  CHAT_MODEL_DEFAULTS: 'chat.modelDefaults',
  // ICECAP infra-safety gate opt-ins (both default off). Consumed by
  // buildKasSettings() via boolMappings. Monitor evaluates + warns; enforce blocks.
  CHAT_ENABLE_INFRA_SAFETY_MONITOR: 'chat.enableInfraSafetyMonitor',
  CHAT_ENABLE_INFRA_SAFETY_ENFORCE: 'chat.enableInfraSafetyEnforce',
  // Whether the workflow monitor enables mouse capture (click-to-select nodes,
  // wheel scroll, drag-resize) on open. Default ON; toggled live with `m` and
  // persisted so power users who rely on terminal text-selection can opt out.
  WORKFLOW_MONITOR_MOUSE: 'chat.workflowMonitor.mouseEnabled',
} as const;

/**
 * Description for the "Display" settings entry, shared by the typed
 * `/settings display` subcommand router (settings-subcommands.ts) and the
 * shared settings panel model (settings-panel-model.ts) so both advertise the
 * same controls — including the "Default UI at startup" (chat.ui.mode) toggle,
 * which lives in the Display sub-panel for both lite and TUI.
 */
export const DISPLAY_SETTINGS_DESCRIPTION =
  'Default UI at startup, animations, ASCII art, icons, and thinking';
