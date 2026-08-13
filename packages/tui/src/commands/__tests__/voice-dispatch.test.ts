import { describe, it, expect, mock, afterAll } from 'bun:test';
import type {
  ModelDownloadInfo,
  PTTSession,
  VoiceHelperCallbacks,
} from '../voice-helper';

/**
 * Controllable stand-in for the voice subprocess. Each call to
 * `startPTTRecording` records its callbacks and exposes a deferred `text`
 * promise the test resolves/rejects to simulate: transcript, no-speech,
 * needs-download, and download-failure outcomes.
 */
interface FakeSession {
  callbacks: VoiceHelperCallbacks | undefined;
  confirmDownload: boolean;
  resolve: (text: string | null) => void;
  reject: (err: Error) => void;
  session: PTTSession;
}

const sessions: FakeSession[] = [];

const startPTTRecordingMock = mock(
  (
    _remoteServerUrl?: string,
    callbacks?: VoiceHelperCallbacks,
    _ptt?: boolean,
    confirmDownload = false
  ): PTTSession => {
    let resolve!: (text: string | null) => void;
    let reject!: (err: Error) => void;
    const text = new Promise<string | null>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    const session: PTTSession = { stop: () => {}, cancel: () => {}, text };
    sessions.push({ callbacks, confirmDownload, resolve, reject, session });
    return session;
  }
);

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../voice-helper']);

mock.module('../voice-helper', () => ({
  startPTTRecording: startPTTRecordingMock,
}));

afterAll(() => {
  mock.restore();
});

import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { createMockCommandContext } from './test-helpers.js';

function voiceCmd(): SlashCommand {
  return {
    name: '/voice',
    description: 'voice',
    source: 'local',
    meta: { local: true },
  } as SlashCommand;
}

const DOWNLOAD_INFO: ModelDownloadInfo = {
  model: 'ggml-base.bin',
  sizeMb: 148,
  license: 'MIT',
  licenseUrl: 'https://github.com/openai/whisper/blob/main/LICENSE',
};

function latest(): FakeSession {
  return sessions[sessions.length - 1]!;
}

describe('/voice dispatch', () => {
  it('surfaces the download confirm gate on needs_download instead of recording text', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    const s = latest();
    // Subprocess reports the model is missing, then exits without a transcript.
    s.callbacks?.onNeedsDownload?.(DOWNLOAD_INFO);
    s.resolve(null);
    await dispatched;

    expect(ctx._spies.setVoiceDownloadConfirm!).toHaveBeenCalled();
    const arg = ctx._spies.setVoiceDownloadConfirm!.mock.calls.at(-1)![0];
    expect(arg.info).toEqual(DOWNLOAD_INFO);
    // A needs_download round is terminal — no "No speech detected".
    expect(ctx._spies.setPendingVoiceText!).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert!).not.toHaveBeenCalledWith(
      'No speech detected',
      'error',
      2000
    );
  });

  it('confirming the gate re-runs voice capture with confirmDownload=true', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    const first = latest();
    first.callbacks?.onNeedsDownload?.(DOWNLOAD_INFO);
    first.resolve(null);
    await dispatched;

    // Fire the gate's onConfirm — it should clear the gate and re-spawn.
    const gate = ctx._spies.setVoiceDownloadConfirm!.mock.calls.at(-1)![0];
    gate.onConfirm();
    const second = latest();
    expect(second.confirmDownload).toBe(true);
    // Gate cleared (last call is null).
    expect(
      ctx._spies.setVoiceDownloadConfirm!.mock.calls.at(-1)![0]
    ).toBeNull();

    // Let the re-run finish so no promise dangles.
    second.callbacks?.onStatus?.('download_complete');
    second.resolve(null);
  });

  it('declining the gate clears it and warns the user', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    const first = latest();
    first.callbacks?.onNeedsDownload?.(DOWNLOAD_INFO);
    first.resolve(null);
    await dispatched;

    const gate = ctx._spies.setVoiceDownloadConfirm!.mock.calls.at(-1)![0];
    gate.onDecline();
    expect(
      ctx._spies.setVoiceDownloadConfirm!.mock.calls.at(-1)![0]
    ).toBeNull();
    const warned = ctx._spies.showAlert!.mock.calls.some(
      (c: any[]) => c[1] === 'warning'
    );
    expect(warned).toBe(true);
  });

  it('surfaces a download failure as an error alert (not "no speech")', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    latest().reject(new Error('Model download failed: network down'));
    await dispatched;

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'Model download failed: network down',
      'error',
      3000
    );
    // UI teardown still runs via the finally.
    expect(ctx._spies.setVoiceLevel!).toHaveBeenCalledWith(null);
    expect(ctx._spies.setVoicePartialText!).toHaveBeenCalledWith(null);
  });

  it('routes a transcript into pending input when auto-submit is off', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    latest().resolve('refactor the parser');
    await dispatched;

    expect(ctx._spies.setPendingVoiceText!).toHaveBeenCalledWith(
      'refactor the parser'
    );
    expect(ctx._spies.sendMessage!).not.toHaveBeenCalled();
  });

  it('shows "No speech detected" when the round returns no text', async () => {
    sessions.length = 0;
    startPTTRecordingMock.mockClear();
    const ctx = createMockCommandContext();

    const dispatched = dispatch(voiceCmd(), '', ctx);
    latest().resolve(null);
    await dispatched;

    expect(ctx._spies.showAlert!).toHaveBeenCalledWith(
      'No speech detected',
      'error',
      2000
    );
  });
});
