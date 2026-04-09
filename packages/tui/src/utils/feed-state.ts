import { join } from 'path';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { homedir } from 'os';
import { logger } from './logger.js';
import { getCliVersion } from './version.js';
import type { AnnouncementEntry } from '../constants/feed.js';

function statePath(): string {
  const home = process.env.HOME || homedir();
  return join(home, '.kiro', 'settings', 'feed_state.json');
}

/**
 * Version match: returns true if cliVersion >= entryVersion.
 * Supports X wildcards in entryVersion: "2.X.X" matches any 2.x.x,
 * "2.0.X" matches any 2.0.x.
 */
function versionMatch(cliVersion: string, entryVersion: string): boolean {
  const cli = cliVersion.split('.').map(Number);
  if (cli.some(Number.isNaN)) return false;
  const entry = entryVersion.split('.');
  for (let i = 0; i < 3; i++) {
    if (entry[i]?.toUpperCase() === 'X') return true;
    const c = cli[i] ?? 0;
    const e = Number(entry[i] ?? 0);
    if (Number.isNaN(e)) return false;
    if (c > e) return true;
    if (c < e) return false;
  }
  return true;
}

export function getShowCounts(): Record<string, number> {
  try {
    const p = statePath();
    if (existsSync(p)) {
      const raw = JSON.parse(readFileSync(p, 'utf-8'));
      if (raw && typeof raw === 'object' && !Array.isArray(raw)) {
        return raw as Record<string, number>;
      }
    }
  } catch {
    logger.warn('[feed-state] Failed to read feed state, starting fresh');
  }
  return {};
}

export function incrementShowCount(id: string): void {
  try {
    const counts = getShowCounts();
    counts[id] = (counts[id] ?? 0) + 1;
    const p = statePath();
    const dir = join(p, '..');
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    writeFileSync(p, JSON.stringify(counts), 'utf-8');
  } catch (err) {
    logger.warn('[feed-state] Failed to write feed state:', err);
  }
}

/**
 * Returns the active announcement, or null. Only the newest entry (by version)
 * is ever considered — older entries are superseded even if not exhausted.
 * Within the same version, lower priority number wins.
 */
export function getActiveAnnouncement(
  messages: AnnouncementEntry[]
): AnnouncementEntry | null {
  if (messages.length === 0) return null;

  // Pick the single newest entry (version desc, then priority asc)
  const sorted = [...messages].sort((a, b) => {
    const va = a.version.replace(/[xX]/g, '0').split('.').map(Number);
    const vb = b.version.replace(/[xX]/g, '0').split('.').map(Number);
    for (let i = 0; i < 3; i++) {
      if ((vb[i] ?? 0) !== (va[i] ?? 0)) return (vb[i] ?? 0) - (va[i] ?? 0);
    }
    return a.priority - b.priority;
  });

  const candidate = sorted[0];
  if (!candidate) return null;
  const counts = getShowCounts();
  const cliVersion = getCliVersion();

  if (
    candidate.maxShowCount > 0 &&
    (counts[candidate.id] ?? 0) < candidate.maxShowCount &&
    versionMatch(cliVersion, candidate.version)
  ) {
    return candidate;
  }

  return null;
}
