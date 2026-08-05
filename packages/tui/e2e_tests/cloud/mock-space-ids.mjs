// Single source of truth for the mock BFF's stable space ids, imported by
// BOTH mock-bff.mjs and CloudTestCase.ts so there is no hand-kept "keep in
// sync" contract to drift (review of PR #3710). Kept as a side-effect-free
// .mjs (no server boot, no TS-only syntax) so the plain-node mock and the
// bun/TS harness can both import it.
//
// UUID-shaped so the CLI's `--resume-id` full-id fast path (isFullSessionId,
// `^[0-9a-f]{8}-[0-9a-f]{4}`) accepts them; spaceId === sessionId today.
//  - banana:  idle, bound to kiro-team/banana-service; replays canned history
//             under MOCK_BFF_HISTORY=1.
//  - empty:   idle "New cloud sandbox" with no bound repo.
//  - working: mid-turn (isExecuting, no question) — listed under
//             MOCK_BFF_CONCURRENT=1 only.
//  - waiting: blocked on a pending question — listed under
//             MOCK_BFF_CONCURRENT=1 only.
//  - created: what CreateSpace returns — a fresh space with no canned
//             history, kept distinct so a create never replays another
//             session's transcript.
export const MOCK_SPACE_IDS = {
  banana: 'aaaaaaa1-0001-4001-8001-000000000001',
  empty: 'aaaaaaa2-0002-4002-8002-000000000002',
  working: 'aaaaaaa3-0003-4003-8003-000000000003',
  waiting: 'aaaaaaa4-0004-4004-8004-000000000004',
  created: 'aaaaaaa5-0005-4005-8005-000000000005',
};
