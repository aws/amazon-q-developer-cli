/**
 * Setting keys matching the dotted names in the Rust settings store.
 * Source of truth: crates/chat-cli-v2/src/database/settings.rs
 *
 * TODO: update when chat-cli and chat-cli-v2 settings are merged eventually
 */
export const Settings = {
  CHAT_GREETING_ENABLED: 'chat.greeting.enabled',
} as const;
