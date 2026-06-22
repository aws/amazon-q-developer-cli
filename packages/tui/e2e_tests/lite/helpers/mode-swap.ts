import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import { CMD_LITE, CMD_TUI, typeSlashCommand } from './commands';

export async function switchToLite(tc: E2ETestCase | TestCase): Promise<void> {
  await typeSlashCommand(tc, CMD_LITE, {
    trailingSpace: true,
    postEnterMs: 800,
  });
}

export async function switchToTui(tc: E2ETestCase | TestCase): Promise<void> {
  await typeSlashCommand(tc, CMD_TUI, {
    trailingSpace: true,
    postEnterMs: 800,
  });
}
