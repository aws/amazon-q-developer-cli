/**
 * Pins the hermetic-runner contract: rendering branches on these variables and
 * spawned PTY children inherit the runner's environment, so a suite that
 * inherits them from a developer's multiplexer session exercises a different
 * code path than CI. Fails if the preload is dropped from bunfig.
 */

import { describe, expect, it } from 'bun:test';

describe('multiplexer env preload', () => {
  it('leaves no multiplexer markers in the environment', () => {
    expect(process.env.TMUX).toBeUndefined();
    expect(process.env.ZELLIJ).toBeUndefined();
    expect(process.env.TWINKI_HARDWARE_CURSOR).toBeUndefined();
  });
});
