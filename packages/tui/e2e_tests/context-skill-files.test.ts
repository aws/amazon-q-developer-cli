/**
 * E2E test: /context should display skill:// resources (SKILL.md files).
 *
 * Regression test for a bug where calculate_context_files_tokens only
 * stripped the file:// prefix but not skill://, causing skill files to
 * silently disappear from the context breakdown.
 */

import { afterEach, describe, it } from 'bun:test';
import fs from 'fs';
import path from 'path';
import { E2ETestCase } from './E2ETestCase';

describe('Context skill files', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    await new Promise(resolve => setTimeout(resolve, 500));
  });

  it('shows skill files in /context breakdown', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('context-skill-files')
      .launch();

    // Create a skill file inside the sandbox HOME (~/.kiro/skills/test-skill/SKILL.md)
    const homeDir = (testCase as any).sandboxDir;
    const skillDir = path.join(homeDir, '.kiro', 'skills', 'test-skill');
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '# Test Skill\nThis is a test skill for E2E.');

    await testCase.waitForText('ask a question', 10000);
    await testCase.waitForSlashCommands();
    await testCase.sleepMs(500);

    // Run /context show (expanded view shows individual files)
    const cmd = '/context show';
    for (const char of cmd) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(200);
    await testCase.sendKeys('\r');
    await testCase.sleepMs(1500);

    // The skill file should appear in the context breakdown
    await testCase.waitForText('SKILL.md', 10000);

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});
