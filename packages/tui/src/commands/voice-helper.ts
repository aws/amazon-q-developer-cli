/**
 * Voice helper - spawns the kiro-cli binary in voice-only mode.
 *
 * The Rust binary handles:
 * - Microphone recording (via cpal)
 * - Local whisper transcription (via whisper-rs)
 *
 * When stdout is piped (TUI mode), it outputs JSON lines:
 *   {"type":"status","value":"recording"}
 *   {"type":"level","value":5}
 *   {"type":"text","value":"hello world"}
 *
 * If a remote voice server URL is configured (KIRO_VOICE_SERVER_URL), the
 * local binary is skipped entirely and the TUI streams from the remote
 * server's /voice/record/stream SSE endpoint instead.
 */

import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { logger } from '../utils/logger.js';

/**
 * Find the kiro-cli binary path.
 * Throws a descriptive error if the binary cannot be found.
 */
function getBinaryPath(): string {
  const path = process.env.KIRO_CHAT_CLI_BIN ?? 'kiro-cli';
  // Validate absolute/relative paths exist on disk
  if (path.includes('/') && !existsSync(path)) {
    throw new Error(
      `Voice binary not found at "${path}". Check KIRO_CHAT_CLI_BIN or reinstall kiro-cli.`
    );
  }
  return path;
}

/** Details of a model that must be downloaded before voice can run. */
export interface ModelDownloadInfo {
  model: string;
  sizeMb: number;
  license: string;
  licenseUrl: string;
}

/** Structured failure emitted by the voice subprocess (e.g. model download failed). */
export interface VoiceErrorInfo {
  code: string;
  message: string;
}

export interface VoiceHelperCallbacks {
  onLevel?: (level: number) => void;
  onStatus?: (status: string) => void;
  onPartial?: (text: string) => void;
  /** Fired when the speech model is missing and needs the user to confirm a download. */
  onNeedsDownload?: (info: ModelDownloadInfo) => void;
}

/**
 * Overrides for process-level collaborators. The spawner is injectable because
 * the alternative — replacing `child_process` with a module mock — swaps it for
 * every suite sharing the test process, so any other suite that spawns for real
 * would wait on a child that never reports.
 */
export interface VoiceHelperDeps {
  spawn?: typeof spawn;
}

export interface PTTSession {
  /** Send the stop signal (does not wait for transcription). */
  stop: () => void;
  /** Kill the subprocess immediately — discards any pending transcription. */
  cancel: () => void;
  /** Resolves with transcribed text (or null) once recording + transcription finishes. */
  text: Promise<string | null>;
}

/**
 * Stream from the remote voice server's /voice/record/stream SSE endpoint.
 * Fires callbacks for activity levels and returns final text.
 */
function startRemoteRecording(
  serverUrl: string,
  callbacks?: VoiceHelperCallbacks
): PTTSession {
  let resolveText: ((t: string | null) => void) | null = null;
  let rejectText: ((e: Error) => void) | null = null;
  const textPromise = new Promise<string | null>((res, rej) => {
    resolveText = res;
    rejectText = rej;
  });

  const abortController = new AbortController();
  let settled = false;

  const url = `${serverUrl.replace(/\/$/, '')}/voice/record/stream`;
  logger.debug('[voice] connecting to remote voice server SSE:', url);

  // Signal recording started immediately
  callbacks?.onStatus?.('recording');

  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 49 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line sonarjs/cognitive-complexity
  (async () => {
    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({}),
        signal: abortController.signal,
      });

      if (!response.ok) {
        throw new Error(`Voice server returned ${response.status}`);
      }

      if (!response.body) {
        throw new Error('Voice server returned no body');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          if (!line.startsWith('data: ')) continue;
          const json = line.slice(6).trim();
          if (!json) continue;

          try {
            const event = JSON.parse(json) as {
              type: string;
              level?: number;
              text?: string | null;
              message?: string;
            };

            switch (event.type) {
              case 'activity':
                // LINT-DEBT(max-depth): pre-existing at gate adoption; Blocks are nested too deeply (6). Maximum allowed is 5.; refactor before extending
                // eslint-disable-next-line max-depth
                if (event.level !== undefined) {
                  callbacks?.onLevel?.(event.level);
                }
                break;
              case 'done':
                // LINT-DEBT(max-depth): pre-existing at gate adoption; Blocks are nested too deeply (6). Maximum allowed is 5.; refactor before extending
                // eslint-disable-next-line max-depth
                if (!settled) {
                  settled = true;
                  resolveText!(event.text?.trim() || null);
                }
                return;
              case 'error':
                // LINT-DEBT(max-depth): pre-existing at gate adoption; Blocks are nested too deeply (6). Maximum allowed is 5.; refactor before extending
                // eslint-disable-next-line max-depth
                if (!settled) {
                  settled = true;
                  rejectText!(new Error(event.message ?? 'Remote voice error'));
                }
                return;
            }
          } catch {
            logger.debug('[voice] ignoring non-JSON SSE line:', json);
          }
        }
      }

      // Stream ended without done/error event
      if (!settled) {
        settled = true;
        resolveText!(null);
      }
    } catch (err) {
      if (!settled) {
        settled = true;
        if (abortController.signal.aborted) {
          resolveText!(null);
        } else {
          const msg =
            err instanceof Error &&
            (err.message.includes('ECONNREFUSED') ||
              err.message.includes('socket') ||
              err.message.includes('connect'))
              ? 'Voice server not reachable. Run `kiro-cli voice-cloud-setup <hostname>` on your local machine first.'
              : err instanceof Error
                ? err.message
                : 'Remote voice failed';
          rejectText!(new Error(msg));
        }
      }
    }
  })();

  const stop = () => {
    const stopUrl = `${serverUrl.replace(/\/$/, '')}/voice/record/stop`;
    fetch(stopUrl, { method: 'POST' }).catch(() => {});
  };

  const cancel = () => {
    logger.debug('[voice] remote cancel — aborting connection');
    abortController.abort();
  };

  return { stop, cancel, text: textPromise };
}

/**
 * Push-to-talk: start recording immediately.
 * Call `session.stop()` to end recording and get the transcription.
 *
 * When remoteServerUrl is set, skips local binary entirely and streams
 * from the remote voice server.
 */
export function startPTTRecording(
  remoteServerUrl?: string,
  callbacks?: VoiceHelperCallbacks,
  ptt = true,
  confirmDownload = false,
  deps: VoiceHelperDeps = {}
): PTTSession {
  // When a remote server is configured, use it directly — skip local binary
  if (remoteServerUrl) {
    return startRemoteRecording(remoteServerUrl, callbacks);
  }

  const binary = getBinaryPath();
  const args = ptt ? ['voice', '--ptt'] : ['voice'];
  // Only pass --confirm-download after the user has accepted the model download.
  if (confirmDownload) args.push('--confirm-download');
  logger.debug('[voice] spawning voice helper:', binary, args);

  const child = (deps.spawn ?? spawn)(binary, args, {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  // Log subprocess stderr instead of letting it bleed into the TUI display
  child.stderr?.on('data', (data: Buffer) => {
    logger.debug('[voice] stderr:', data.toString().trimEnd());
  });

  let stdout = '';
  let finalText: string | null = null;
  // Set when the subprocess emits a structured `error` event; the close handler
  // rejects with this so callers surface a real failure (e.g. download failed)
  // instead of treating a silent non-zero exit as "no speech detected".
  let voiceError: VoiceErrorInfo | null = null;
  let resolveText: ((t: string | null) => void) | null = null;
  let rejectText: ((e: Error) => void) | null = null;

  const textPromise = new Promise<string | null>((res, rej) => {
    resolveText = res;
    rejectText = rej;
  });

  const parseStdoutChunk = (chunk: string) => {
    stdout += chunk;
    const lines = stdout.split('\n');
    stdout = lines.pop() ?? '';
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        const event = JSON.parse(trimmed) as { type: string; value: unknown };
        switch (event.type) {
          case 'level':
            callbacks?.onLevel?.(event.value as number);
            break;
          case 'status':
            callbacks?.onStatus?.(event.value as string);
            break;
          case 'partial':
            if (event.value) callbacks?.onPartial?.(event.value as string);
            break;
          case 'needs_download':
            callbacks?.onNeedsDownload?.(event.value as ModelDownloadInfo);
            break;
          case 'error':
            voiceError = event.value as VoiceErrorInfo;
            break;
          case 'text':
            finalText = (event.value as string | null) ?? null;
            break;
        }
      } catch {
        logger.debug('[voice] ignoring non-JSON stdout line:', trimmed);
      }
    }
  };

  child.stdout?.on('data', (data: Buffer) => parseStdoutChunk(data.toString()));

  let settled = false;
  let cancelled = false;

  child.on('error', (err) => {
    if (settled) return;
    settled = true;
    if (cancelled) {
      resolveText!(null);
      return;
    }
    logger.debug('[voice] PTT helper error:', err.message);
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      rejectText!(
        new Error(
          `Voice binary "${binary}" not found. Check KIRO_CHAT_CLI_BIN or reinstall kiro-cli.`
        )
      );
      return;
    }
    rejectText!(new Error(`Voice helper failed: ${err.message}`));
  });

  child.on('close', (code, signal) => {
    if (settled) return;
    settled = true;
    // Flush remaining buffer
    if (stdout.trim()) {
      try {
        const event = JSON.parse(stdout.trim()) as {
          type: string;
          value: unknown;
        };
        if (event.type === 'text')
          finalText = (event.value as string | null) ?? null;
      } catch {
        logger.debug('[voice] ignoring non-JSON stdout line:', stdout.trim());
      }
    }

    if (cancelled) {
      resolveText!(null);
    } else if (voiceError) {
      rejectText!(new Error(voiceError.message || 'Voice failed'));
    } else if (code === 0) {
      resolveText!(finalText);
    } else if (signal) {
      rejectText!(new Error(`Voice helper terminated by ${signal}`));
    } else {
      rejectText!(
        new Error(`Voice helper exited with code ${code ?? 'unknown'}`)
      );
    }
  });

  const stop = () => {
    // Send Enter to the binary's stdin to stop recording
    try {
      child.stdin?.write('\n');
      child.stdin?.end();
    } catch {
      // stdin may already be closed
    }
  };

  const cancel = () => {
    cancelled = true;
    try {
      child.kill('SIGKILL');
    } catch {
      // process may already be dead
    }
  };

  return { stop, cancel, text: textPromise };
}

/**
 * Spawn the voice helper process and return transcribed text.
 * Returns null if no speech was detected or user cancelled.
 */
export function spawnVoiceHelper(
  remoteServerUrl?: string,
  callbacks?: VoiceHelperCallbacks
): Promise<string | null> {
  const session = startPTTRecording(remoteServerUrl, callbacks, false);
  return session.text;
}
