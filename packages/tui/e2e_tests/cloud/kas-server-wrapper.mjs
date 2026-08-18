// KAS server wrapper for the cloud e2e harness.
//
// The TUI spawns KAS as `node <KIRO_KAS_SERVER_PATH> --transport=stdio ...`
// with a fixed argument list — there is no pass-through for extra KAS flags.
// KAS reads its control-plane endpoint (governance, model list) only from the
// `--control-plane-endpoint` argv flag, and since 0.46 it settles a
// registry-served model before every prompt/spec operation
// (kiro-team/kiro-agent#2109 fails closed on a cold registry). The offline
// harness therefore needs that flag to point at the per-test mock BFF, which
// serves `GET /List-Available-Models`.
//
// This wrapper is what `CloudHarness` sets as KIRO_KAS_SERVER_PATH: it appends
// the flag from KIRO_TEST_CONTROL_PLANE_ENDPOINT (set by the harness to the
// mock BFF's URL) and then loads the real server, which parses process.argv
// itself. Test infrastructure only — production spawns the real server path.
/* global process, URL */
const endpoint = process.env.KIRO_TEST_CONTROL_PLANE_ENDPOINT;
if (endpoint) {
  process.argv.push(`--control-plane-endpoint=${endpoint}`);
}
await import(
  new URL(
    '../../node_modules/@kiro/agent/dist/server/acp-server.js',
    import.meta.url
  ).pathname
);
