import { describe, expect, it } from 'bun:test';
import { runStopNotice } from '../run-stop-notice.js';

describe('runStopNotice', () => {
  it('attributes a user-initiated stop with its reason', () => {
    expect(
      runStopNotice({
        status: 'paused',
        stopInitiator: 'user',
        stopReason: 'wrong branch',
      })
    ).toBe('Stopped by you: wrong branch');
  });

  it('still attributes a user stop that carried no reason', () => {
    expect(runStopNotice({ status: 'paused', stopInitiator: 'user' })).toBe(
      'Stopped by you.'
    );
    expect(
      runStopNotice({
        status: 'paused',
        stopInitiator: 'user',
        stopReason: '  ',
      })
    ).toBe('Stopped by you.');
  });

  it('falls back to the pause reason when the run stopped on its own', () => {
    expect(
      runStopNotice({ status: 'paused', pauseReason: 'waiting on approval' })
    ).toBe('waiting on approval');
  });

  it('shows nothing when there is nothing to explain', () => {
    expect(runStopNotice({ status: 'paused' })).toBeNull();
    expect(runStopNotice({ status: 'paused', pauseReason: '   ' })).toBeNull();
  });

  it('explains nothing about a run that is moving', () => {
    // KAS keeps the attribution across a resume, so a snapshot restoring a live
    // run carries a stale `stopInitiator` no later event will clear.
    expect(
      runStopNotice({
        status: 'running',
        stopInitiator: 'user',
        stopReason: 'wrong branch',
      })
    ).toBeNull();
    expect(
      runStopNotice({ status: 'running', pauseReason: 'paused before review' })
    ).toBeNull();
  });
});
