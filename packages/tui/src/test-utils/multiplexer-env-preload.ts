/**
 * Test preload: drop multiplexer markers from the environment.
 *
 * Rendering branches on `TMUX`/`ZELLIJ` (the terminal draws its own cursor
 * there), and spawned PTY children inherit this process's environment. Without
 * this, a suite run from inside tmux exercises a different code path than CI and
 * assertions pass or fail depending on where they run. Tests that want the
 * multiplexer path set these vars themselves.
 */

for (const key of ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR']) {
  delete process.env[key];
}
