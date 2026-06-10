# ACP Integration Test Coverage Matrix

## Summary

**37 tests** across **10 files** covering the KAS ↔ CLI ACP integration surface.
All tests pass. 3 product bugs found and fixed.

---

## Extension Methods (Client → Agent)

| Extension Method                     | Test File                    | Scenarios                              | Status |
| ------------------------------------ | ---------------------------- | -------------------------------------- | ------ |
| `_kiro/session/context` (show)       | `context-command.test.ts`    | Sends correct params, receives entries | ✅     |
| `_kiro/session/context` (add)        | `context-command.test.ts`    | Sends path + sessionId                 | ✅     |
| `_kiro/session/context` (remove)     | `context-command.test.ts`    | Sends path + sessionId                 | ✅     |
| `_kiro/session/context` (clear)      | `context-command.test.ts`    | Sends sessionId                        | ✅     |
| `_kiro/session/compact`              | `compact-command.test.ts`    | Sends sessionId                        | ✅     |
| `_kiro/account/getUsage`             | `usage-command.test.ts`      | Sends sessionId, receives quota        | ✅     |
| `_kiro/codeIntelligence` (status)    | `code-commands.test.ts`      | Subcommand routing                     | ✅     |
| `_kiro/codeIntelligence` (init)      | `code-commands.test.ts`      | Subcommand routing                     | ✅     |
| `_kiro/codeIntelligence` (overview)  | `code-commands.test.ts`      | Subcommand routing                     | ✅     |
| `_kiro/hooks/list`                   | `hooks-did-change.test.ts`   | Fallback pull path                     | ✅     |
| `_kiro/knowledge` (show)             | `knowledge-command.test.ts`  | Existing test                          | ✅     |
| `session/set_config_option` (model)  | `model-switch.test.ts`       | /model picker selection                | ✅     |
| `session/set_config_option` (mode)   | `agent-flag.test.ts`         | --agent CLI flag                       | ✅     |
| `session/set_config_option` (effort) | `effort-command.test.ts`     | Existing test                          | ✅     |
| `session/cancel`                     | `model-switch.test.ts`       | Ctrl+C during processing               | ✅     |
| `session/request_permission`         | `permission-request.test.ts` | Approve + Deny flows                   | ✅     |

## Notifications (Agent → Client)

| Notification                                    | Test File                     | Scenarios                          | Status |
| ----------------------------------------------- | ----------------------------- | ---------------------------------- | ------ |
| `_kiro/hooks/didChange`                         | `hooks-did-change.test.ts`    | Populates store hooksList          | ✅     |
| `_kiro/customAgent/not_found`                   | `agent-notifications.test.ts` | initErrors + currentAgent fallback | ✅     |
| `_kiro/customAgent/config_error`                | `agent-notifications.test.ts` | Error surfaces in TUI              | ✅     |
| `_kiro/error/rate_limit`                        | `agent-notifications.test.ts` | transientAlert populated           | ✅     |
| `_kiro/mcp/governance_disabled`                 | `agent-notifications.test.ts` | Handled without crash              | ✅     |
| `_kiro/mcp/status` (OAuth)                      | `mcp-oauth.test.ts`           | Existing test                      | ✅     |
| `_kiro/mcp/status` (servers online)             | `mcp-status.test.ts`          | /mcp shows servers + tools         | ✅     |
| `agent_thought_chunk`                           | `mcp-status.test.ts`          | Thinking text renders on screen    | ✅     |
| `current_mode_update`                           | `agent-notifications.test.ts` | store.currentAgent updated         | ✅     |
| `config_option_update`                          | `effort-command.test.ts`      | Existing test                      | ✅     |
| `available_commands_update`                     | `prompts-command.test.ts`     | Existing test                      | ✅     |
| `session_info_update` (summarization_started)   | `compact-command.test.ts`     | isCompacting=true                  | ✅     |
| `session_info_update` (summarization_completed) | `compact-command.test.ts`     | isCompacting=false                 | ✅     |
| `session_info_update` (summarization_failed)    | `compact-command.test.ts`     | isCompacting=false                 | ✅     |
| `session_info_update` (context_usage)           | `permission-request.test.ts`  | contextUsagePercent updated        | ✅     |
| `session_info_update` (turn_completion)         | `permission-request.test.ts`  | isProcessing=false                 | ✅     |

## Tool Rendering (KAS Tools via `tool_call` → `tool_call_update`)

| Tool Name                        | KAS Kind  | Test File                | Status        |
| -------------------------------- | --------- | ------------------------ | ------------- |
| `execute_bash`                   | `execute` | `tool-rendering.test.ts` | ✅            |
| `read_file`                      | `read`    | `tool-rendering.test.ts` | ✅            |
| `fs_write`                       | `edit`    | `tool-rendering.test.ts` | ✅            |
| `grep_search`                    | `search`  | `tool-rendering.test.ts` | ✅            |
| `web_fetch`                      | `fetch`   | `tool-rendering.test.ts` | ✅            |
| `invoke_sub_agent`               | `other`   | `tool-rendering.test.ts` | ✅            |
| Tool failure (status: failed)    | any       | `tool-rendering.test.ts` | ✅            |
| Parallel tools (3x out-of-order) | mixed     | `tool-rendering.test.ts` | ✅            |
| MCP tool name/output transform   | —         | `mcp-transform.test.ts`  | ✅ (existing) |

## Advanced Interaction Scenarios

| Scenario                                    | Test File                    | Status        |
| ------------------------------------------- | ---------------------------- | ------------- |
| Permission approve (Enter)                  | `permission-request.test.ts` | ✅            |
| Permission deny (Escape)                    | `permission-request.test.ts` | ✅            |
| Cancel mid-processing (Ctrl+C)              | `model-switch.test.ts`       | ✅            |
| Cancel mid-tool → new prompt recovery       | `model-switch.test.ts`       | ✅            |
| /agent swap → server confirms mode switch   | `model-switch.test.ts`       | ✅            |
| Subagent tool events routed to main store   | `tool-rendering.test.ts`     | ✅            |
| Steering docs via available_commands_update | `prompts-command.test.ts`    | ✅ (existing) |

## Bugs Found and Fixed

| Bug                                     | Root Cause                                       | Fix                                      | Validated By                  |
| --------------------------------------- | ------------------------------------------------ | ---------------------------------------- | ----------------------------- |
| HooksUpdate dropped at idle             | `kiro.ts` global handler missing forwarding      | Added to `initNotificationHandler` block | `hooks-did-change.test.ts`    |
| RateLimitError dropped at idle          | Same as above                                    | Same fix                                 | `agent-notifications.test.ts` |
| `not_found` doesn't update currentAgent | `acp-client.ts` didn't broadcast `AgentSwitched` | Added broadcast after cache update       | `agent-notifications.test.ts` |

## KAS Kind Reference (from `kiro-agent/src/acp/tool-call-emitter.ts`)

| KAS Kind  | Tools                                         | Notes                |
| --------- | --------------------------------------------- | -------------------- |
| `read`    | read_file, read_files                         | Has `locations`      |
| `edit`    | fs_write, str_replace, fs_append              | Has `locations`      |
| `delete`  | delete_file                                   | Has `locations`      |
| `search`  | grep_search, file_search                      | Optional `locations` |
| `fetch`   | web_fetch                                     | Optional `locations` |
| `execute` | execute_bash, execute_pwsh                    | No `locations`       |
| `other`   | knowledge, invoke_sub_agent, introspect, etc. | No `locations`       |

**Note:** TUI `ToolKind` type includes `'shell'` but KAS never sends it. KAS sends `'execute'`.
