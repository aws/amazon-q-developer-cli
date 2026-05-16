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
  CHAT_ASCII_MODE: 'chat.allowAsciiArt',
  CHAT_ANIMATIONS: 'chat.allowAnimations',
  CHAT_ICONS: 'chat.allowIcons',
  CHAT_SHOW_THINKING: 'chat.showThinking',
} as const;
