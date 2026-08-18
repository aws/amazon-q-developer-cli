# ACP Integration Coverage

ACP integration coverage is reported as **capability coverage**, not source-code
line coverage.

The suite runs the TUI in a spawned Bun process. Bun's `--coverage` output only
instruments the parent test process, so its `All files` percentage describes the
test harness and must not be published as TUI source coverage. Bun 1.3.14 also
does not emit child coverage through `NODE_V8_COVERAGE`.

## Metric

`coverage-manifest.json` declares the denominator:

- ACP session operations and updates
- KAS extension requests and notifications
- workflow requests and lifecycle events
- critical workflow UI journeys

A capability is covered when the manifest names an exact ACP integration
testcase as evidence. The suite writes a Bun JUnit report, and the reporter
requires every referenced `file#test name` to appear as passed before reporting
overall, per-area, and uncovered capability counts.

Run the suite and report:

```bash
bun run test:acp-integ
bun run report:acp-integ-coverage
```

Reports are written to:

```text
integ_tests/test-outputs/coverage/acp-integration.json
integ_tests/test-outputs/coverage/acp-integration.md
```

RC Certification publishes the Linux report in its summary and artifact bundle.
The reporter is manifest-driven so smoke and scenario lanes can use the same
schema without pretending that protocol coverage is source coverage.

## Bugs Found and Fixed

| Bug                                     | Root Cause                                       | Fix                                      | Validated By                  |
| --------------------------------------- | ------------------------------------------------ | ---------------------------------------- | ----------------------------- |
| HooksUpdate dropped at idle             | `kiro.ts` global handler missing forwarding      | Added to `initNotificationHandler` block | `hooks-did-change.test.ts`    |
| RateLimitError dropped at idle          | Same as above                                    | Same fix                                 | `agent-notifications.test.ts` |
| `not_found` doesn't update currentAgent | `acp-client.ts` didn't broadcast `AgentSwitched` | Added broadcast after cache update       | `agent-notifications.test.ts` |

## KAS Kind Reference

| KAS Kind  | Tools                                         | Notes                |
| --------- | --------------------------------------------- | -------------------- |
| `read`    | read_file, read_files                         | Has `locations`      |
| `edit`    | fs_write, str_replace, fs_append              | Has `locations`      |
| `delete`  | delete_file                                   | Has `locations`      |
| `search`  | grep_search, file_search                      | Optional `locations` |
| `fetch`   | web_fetch                                     | Optional `locations` |
| `execute` | execute_bash, execute_pwsh                    | No `locations`       |
| `other`   | knowledge, invoke_sub_agent, introspect, etc. | No `locations`       |

TUI `ToolKind` includes `shell`, but KAS sends `execute` for shell tools.

## Source Coverage Follow-up

Source line coverage requires instrumentation inside the spawned TUI process,
followed by source-map-aware merging across processes. Until Bun exposes that
data reliably, the suite must not reuse the parent-process coverage percentage.
