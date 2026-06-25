import type { TestCase } from '../../src/test-utils/TestCase';
import { CMD_LITE, CMD_TUI, typeSlashCommand } from './commands';

export async function switchToLite(tc: TestCase): Promise<void> {
  await typeSlashCommand(tc, CMD_LITE, {
    trailingSpace: true,
    postEnterMs: 800,
  });
}

export async function switchToTui(tc: TestCase): Promise<void> {
  await typeSlashCommand(tc, CMD_TUI, {
    trailingSpace: true,
    postEnterMs: 800,
  });
}

export function visibleIndex(snapshot: string[], marker: string): number {
  return snapshot.findIndex((line) => line.includes(marker));
}

export function visibleCount(snapshot: string[], marker: string): number {
  return snapshot.filter((line) => line.includes(marker)).length;
}
