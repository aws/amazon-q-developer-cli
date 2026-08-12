# session-convert e2e tests

End-to-end tests for cross-engine session resume / load. Each test
spawns the real TUI under a real PTY via `AcpTestCase`, mocks the KAS
side of the ACP wire, and lets the TUI shell out to the real
`target/debug/chat_cli` binary for `--list-sessions`,
`ensure-session`, `import-session`, and `derive-messages`.

## Coverage

`--resume` (most-recent winner across V1 + V2 + KAS):
- `resume-cross-engine.test.ts` - merged listing -> winner -> ensure-session -> session/load
- `with-compaction.test.ts` - same path, but the V2 source contains a `/compact` (`LogEntry::Compaction`)

`--resume-id <id>`:
- `resume-id.test.ts` - V2 UUID resumed in KAS mode
- `resume-id-v1.test.ts` - V1 (classic SQLite) id resumed in KAS mode

`/chat` picker:
- `chat-picker-cross-engine.test.ts` - merged listing in the picker, cross-engine selection round-trips through ensure-session

`/chat load <path>`:
- `chat-load-v2-json.test.ts` - `kiro-session-export-v1` JSON loaded into KAS via `import-session`

Lower-level coverage of the binaries the above depend on:
- `ensure-session-v2-target.test.ts` - `chat _ ensure-session --target-format v2`
- `derive-messages.test.ts` - `chat _ derive-messages` (used as the V2-side expected output for converter assertions)

Fixture-specific schema assertions (decoupled from any one flow):
- `basic-fs-tools.test.ts` - reuses the `--resume-id` setup as the cheapest entry point that drives a real conversion, then asserts the `PersistedMessage[]` shape for the `basic_fs_tools` fixture (1 user + 6 tool_call + 6 tool_result + 1 assistant Say, args verbatim, etc.). The flow tests above that reuse this fixture call `assertBasicFsToolsConverted` from `basic-fs-tools-assertions.ts` to avoid duplicating per-tool checks.

## Test infrastructure

`AcpTestCase` (in `../../src/test-utils/acp-mock/AcpTestCase.ts`) is
reused for every test: it owns the PTY, the in-process mock ACP server,
and the per-test sandbox `KIRO_HOME`. Reuse it for any test that is
purely UI-driven or runs in KAS mode. Tests that need V2 storage on
disk additionally call helpers from `v2_fixtures.ts`.

`assertions.ts` and `v2_fixtures.ts` are shared by the test files in
this directory only.
