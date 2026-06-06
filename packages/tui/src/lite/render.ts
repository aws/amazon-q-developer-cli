/**
 * Re-export barrel for the lite render module.
 * Implementation split into render/*.ts submodules for maintainability.
 * Public API is unchanged — all 30 exports are re-exported here.
 */
export type { RenderTheme } from './render/theme.js';
export { isErrorContent, buildRenderTheme } from './render/theme.js';
export { wrapAnsiLine, wrapAtWords } from './render/text.js';
export {
  renderUserMessage,
  renderAgentMessage,
  renderThinkingBlock,
  renderShellOutputBlock,
} from './render/markdown.js';
export type { ToolCallRenderInfo } from './render/tools.js';
export {
  renderToolCall,
  renderWriteToolCall,
  renderReadToolCall,
  renderLiveStreamingOutputBar,
  extractInlineArg,
  formatToolArgs,
  formatToolArgLines,
  formatTaskToolBody,
} from './render/tools.js';
export {
  formatSubagentApprovalLines,
  renderSubagentFinalBlock,
} from './render/subagent.js';
export type {
  TurnSummaryInfo,
  MessageLike,
  SubagentStageSummary,
  RenderContext,
  VerbosityPreviewKey,
} from './render/message.js';
export {
  renderSystemError,
  renderSystemInfo,
  renderTurnSummary,
  renderMessageToText,
  renderVerbosityPreview,
} from './render/message.js';
