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
import { features, Feature } from '../features';
import { logger } from './logger';

/** The shape sent as clientCapabilities._meta.kiro.settings on initialize. */
export type KasSettings = Record<string, unknown>;

/**
 * Features gated by FeatureManager rollout. When the feature flag is active,
 * the setting is unconditionally sent as `{ enabled: true }` to KAS.
 *
 * To add a new feature-gated setting: append [Feature.Xxx, 'settingKey'].
 * When the feature graduates to GA, move it to `boolMappings` with a
 * user-facing `chat.enableXxx` toggle instead.
 */
const GATED_FEATURES: ReadonlyArray<[Feature, string]> = [
  [Feature.Memory, 'memoryEnable'],
];

/** Apply feature-gated settings to the settings object. */
function applyGatedFeatures(settings: KasSettings): void {
  for (const [feature, settingKey] of GATED_FEATURES) {
    if (features.isEnabled(feature)) {
      settings[settingKey] = { enabled: true };
    }
  }
}

/**
 * Read CLI settings from disk and transform to AgentSettingsSchema format.
 * Returns undefined if no relevant settings are configured.
 */
export function buildKasSettings(): KasSettings | undefined {
  const raw = readCliSettings();
  const settings: KasSettings = {};

  // ─── CLI defaults: tools that were always-on for CLI before settings-driven gating ───
  // These default to enabled unless explicitly disabled by the user.
  // subagentOrchestration is a wire-protocol negotiation: the TUI implements
  // pipeline rendering, so it advertises support to KAS unconditionally.
  // It is intentionally not a user-facing setting. The test-only env
  // override makes a LOCAL KAS register invoke_sub_agent instead (the
  // IDE/cloud default), to manually exercise the invoke-subagent
  // pipeline rendering port without a cloud session.
  const cliDefaults: Record<string, boolean> = {
    codeIntelligence: true,
    knowledge: true,
    thinking: true,
    subagentOrchestration:
      process.env.KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION !== '1',
  };

  // ICECAP monitor mode defaults ON for internal users (rollout-gated).
  // Users can still opt out via `chat.enableInfraSafetyMonitor: false`.
  if (process.env.KIRO_INFRA_SAFETY_ROLLOUT_ENABLED === '1') {
    cliDefaults.infraSafetyMonitor = true;
  }

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

  // ICECAP infra-safety gate. Monitor defaults ON for internal users (via
  // cliDefaults above); enforce remains opt-in. Users override either in
  // cli.json. Gated to the internal cohort via KIRO_INFRA_SAFETY_ROLLOUT_ENABLED.
  if (process.env.KIRO_INFRA_SAFETY_ROLLOUT_ENABLED === '1') {
    boolMappings.push(
      ['chat.enableInfraSafetyMonitor', 'infraSafetyMonitor'],
      ['chat.enableInfraSafetyEnforce', 'infraSafetyEnforce']
    );
  }

  // C2S (Code-to-Spec) Explore agent gate. Opt-in via cli.json, gated to
  // internal nightly builds via Feature::C2s rollout decision.
  if (features.isEnabled(Feature.C2s)) {
    boolMappings.push(['chat.enableC2s', 'c2s']);
  }

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

  // ─── Feature-gated settings (FeatureManager rollout) ───────────────
  applyGatedFeatures(settings);

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
