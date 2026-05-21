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
 * If local voice capture fails (e.g., no microphone on a cloud desktop),
 * falls back to a remote voice server if configured via voice.serverUrl.
 */

import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { logger } from '../utils/logger.js';

/**
 * Find the kiro-cli binary path.
 * Throws a descriptive error if the binary cannot be found.
 */
function getBinaryPath(): string {
  const path = process.env.KIRO_CLI_PATH ?? 'kiro-cli';
  // Validate absolute/relative paths exist on disk
  if (path.includes('/') && !existsSync(path)) {
    throw new Error(
      `Voice binary not found at "${path}". Check KIRO_CLI_PATH or reinstall kiro-cli.`
    );
  }
  return path;
}

/**
 * Try to record via a remote voice server.
 */
async function tryRemoteVoiceServer(serverUrl: string): Promise<string | null> {
  const url = `${serverUrl.replace(/\/$/, '')}/voice/record`;
  logger.debug('[voice] trying remote voice server:', url);

  const response = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({}),
    signal: AbortSignal.timeout(120_000),
  });

  if (!response.ok) {
    throw new Error(`Voice server returned ${response.status}`);
  }

  const data = (await response.json()) as {
    text?: string;
    error?: string;
  };
  if (data.error) {
    throw new Error(data.error);
  }
  return data.text?.trim() || null;
}

export interface VoiceHelperCallbacks {
  onLevel?: (level: number) => void;
  onStatus?: (status: string) => void;
  onPartial?: (text: string) => void;
}

/**
 * Spawn the voice helper process and return transcribed text.
 * Returns null if no speech was detected or user cancelled.
 * Falls back to remote voice server if local capture fails and serverUrl is set.
 */
export function spawnVoiceHelper(
  remoteServerUrl?: string,
  callbacks?: VoiceHelperCallbacks
): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const binary = getBinaryPath();
    logger.debug('[voice] spawning voice helper:', binary, 'voice');

    const child = spawn(binary, ['voice'], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    child.stderr?.on('data', (data: Buffer) => {
      logger.debug('[voice] stderr:', data.toString().trimEnd());
    });

    let stdout = '';
    let finalText: string | null = null;
    let settled = false;

    child.stdout?.on('data', (data: Buffer) => {
      stdout += data.toString();

      // Parse JSON lines
      const lines = stdout.split('\n');
      stdout = lines.pop() ?? ''; // Keep incomplete last line in buffer

      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed) continue;
        try {
          const event = JSON.parse(trimmed) as {
            type: string;
            value: unknown;
          };
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
            case 'text':
              finalText = (event.value as string | null) ?? null;
              break;
          }
        } catch {
          logger.debug('[voice] ignoring non-JSON stdout line:', trimmed);
        }
      }
    });

    child.on('error', (err) => {
      if (settled) return;
      settled = true;
      logger.debug('[voice] helper error:', err.message);
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        reject(
          new Error(
            `Voice binary "${binary}" not found. Check KIRO_CLI_PATH or reinstall kiro-cli.`
          )
        );
        return;
      }
      if (remoteServerUrl) {
        logger.debug('[voice] falling back to remote voice server');
        tryRemoteVoiceServer(remoteServerUrl).then(resolve).catch(reject);
      } else {
        reject(new Error(`Voice helper failed: ${err.message}`));
      }
    });

    child.on('close', (code) => {
      if (settled) return;
      settled = true;
      logger.debug('[voice] helper exited with code:', code);
      // Process any remaining data in buffer
      if (stdout.trim()) {
        try {
          const event = JSON.parse(stdout.trim()) as {
            type: string;
            value: unknown;
          };
          if (event.type === 'text') {
            finalText = (event.value as string | null) ?? null;
          }
        } catch {
          logger.debug('[voice] ignoring non-JSON stdout line:', stdout.trim());
        }
      }

      if (code === 0 && finalText) {
        resolve(finalText);
      } else if (code !== 0 && remoteServerUrl) {
        logger.debug('[voice] local voice failed, trying remote server');
        tryRemoteVoiceServer(remoteServerUrl).then(resolve).catch(reject);
      } else {
        resolve(null);
      }
    });
  });
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
 * Push-to-talk: start recording immediately.
 * Call `session.stop()` to end recording and get the transcription.
 * The binary's stdin is piped so we can send Enter to stop it programmatically.
 */
export function startPTTRecording(
  remoteServerUrl?: string,
  callbacks?: VoiceHelperCallbacks,
  ptt = true
): PTTSession {
  const binary = getBinaryPath();
  const args = ptt ? ['voice', '--ptt'] : ['voice'];
  logger.debug('[voice] spawning voice helper:', binary, args);

  const child = spawn(binary, args, {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  // Log subprocess stderr instead of letting it bleed into the TUI display
  child.stderr?.on('data', (data: Buffer) => {
    logger.debug('[voice] stderr:', data.toString().trimEnd());
  });

  let stdout = '';
  let finalText: string | null = null;
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

  child.on('error', (err) => {
    if (settled) return;
    settled = true;
    logger.debug('[voice] PTT helper error:', err.message);
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      rejectText!(
        new Error(
          `Voice binary "${binary}" not found. Check KIRO_CLI_PATH or reinstall kiro-cli.`
        )
      );
      return;
    }
    if (remoteServerUrl) {
      tryRemoteVoiceServer(remoteServerUrl)
        .then(resolveText!)
        .catch(rejectText!);
    } else {
      rejectText!(new Error(`Voice helper failed: ${err.message}`));
    }
  });

  child.on('close', (code) => {
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

    if (code === 0 && finalText) {
      resolveText!(finalText);
    } else if (code !== 0 && remoteServerUrl) {
      tryRemoteVoiceServer(remoteServerUrl)
        .then(resolveText!)
        .catch(rejectText!);
    } else {
      resolveText!(null);
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
    // Kill the subprocess — non-zero exit resolves the promise with null
    try {
      child.kill('SIGKILL');
    } catch {
      // process may already be dead
    }
  };

  return { stop, cancel, text: textPromise };
}
