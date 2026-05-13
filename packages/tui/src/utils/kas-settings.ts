/**
 * Transforms flat CLI settings (from ~/.kiro/settings/cli.json) into the
 * AgentSettingsSchema shape expected by kiro-agent on initialize.
 *
 * Every setting becomes `{ enabled: boolean, ...extraFields? }`.
 * Absent keys are omitted (consumer decides default).
 *
 * @see kiro-agent: packages/acp-type-covenant/settings/index.ts
 * @see design: CLIENT-SETTINGS-INTEGRATION.md
 */

import { readCliSettings } from './cli-settings';
import { logger } from './logger';

/** The shape sent as clientCapabilities._meta.kiro.settings on initialize. */
export type KasSettings = Record<string, unknown>;

/**
 * Read CLI settings from disk and transform to AgentSettingsSchema format.
 * Returns undefined if no relevant settings are configured.
 */
export function buildKasSettings(): KasSettings | undefined {
  const raw = readCliSettings();
  const settings: KasSettings = {};

  // ─── CLI defaults: tools that were always-on for CLI before settings-driven gating ───
  // These default to enabled unless explicitly disabled by the user.
  const cliDefaults: Record<string, boolean> = {
    codeIntelligence: true,
    knowledge: true,
    toolSearch: true,
  };

  // ─── Boolean feature flags → { enabled: bool } ─────────────────────
  const boolMappings: Array<[string, string]> = [
    ['chat.enableThinking', 'thinking'],
    ['chat.enableKnowledge', 'knowledge'],
    ['chat.enableCodeIntelligence', 'codeIntelligence'],
    ['chat.enableTodoList', 'todoList'],
    ['chat.enableCheckpoint', 'checkpoint'],
    ['chat.enableTangentMode', 'tangentMode'],
    ['chat.disableAutoCompaction', 'disableAutoCompaction'],
    ['chat.enableSubagent', '_subagent'],
    ['chat.enableDelegate', '_delegate'],
  ];

  for (const [cliKey, agentKey] of boolMappings) {
    const val = raw[cliKey];
    if (typeof val === 'boolean') {
      settings[agentKey] = { enabled: val };
    }
  }

  // Apply CLI defaults for tools not explicitly configured
  for (const [key, defaultEnabled] of Object.entries(cliDefaults)) {
    if (!(key in settings)) {
      settings[key] = { enabled: defaultEnabled };
    }
  }

  // ─── Tool Search (structured) ──────────────────────────────────────
  const tsEnabled = raw['toolSearch.enabled'];
  if (typeof tsEnabled === 'boolean') {
    const ts: Record<string, unknown> = { enabled: tsEnabled };
    const minPct = raw['toolSearch.minPct'];
    if (typeof minPct === 'number') ts.minPct = minPct;
    const minTokens = raw['toolSearch.minTokens'];
    if (typeof minTokens === 'number') ts.minTokens = minTokens;
    settings.toolSearch = ts;
  }

  // ─── Compaction (structured) ───────────────────────────────────────
  const excludePct = raw['compaction.excludeContextWindowPercent'];
  const excludeMsgs = raw['compaction.excludeMessages'];
  if (typeof excludePct === 'number' || typeof excludeMsgs === 'number') {
    const c: Record<string, unknown> = { enabled: true };
    if (typeof excludePct === 'number') c.excludePercent = excludePct;
    if (typeof excludeMsgs === 'number') c.excludeMessages = excludeMsgs;
    settings.compaction = c;
  }

  // ─── Knowledge (structured) ────────────────────────────────────────
  const knowledgeEnabled = raw['chat.enableKnowledge'];
  if (typeof knowledgeEnabled === 'boolean') {
    const k: Record<string, unknown> = { enabled: knowledgeEnabled };
    const includePatterns = raw['knowledge.defaultIncludePatterns'];
    if (Array.isArray(includePatterns)) k.includePatterns = includePatterns;
    const excludePatterns = raw['knowledge.defaultExcludePatterns'];
    if (Array.isArray(excludePatterns)) k.excludePatterns = excludePatterns;
    const maxFiles = raw['knowledge.maxFiles'];
    if (typeof maxFiles === 'number') k.maxFiles = maxFiles;
    const chunkSize = raw['knowledge.chunkSize'];
    if (typeof chunkSize === 'number') k.chunkSize = chunkSize;
    const chunkOverlap = raw['knowledge.chunkOverlap'];
    if (typeof chunkOverlap === 'number') k.chunkOverlap = chunkOverlap;
    const indexType = raw['knowledge.indexType'];
    if (indexType === 'fast' || indexType === 'accurate')
      k.indexType = indexType;
    // Override the simple boolean mapping with the structured version
    settings.knowledge = k;
  }

  if (Object.keys(settings).length === 0) return undefined;

  logger.debug('[kas-settings] Built settings for initialize:', settings);
  return settings;
}
