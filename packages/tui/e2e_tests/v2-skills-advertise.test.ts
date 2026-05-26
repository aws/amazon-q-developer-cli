/**
 * E2E test: a SKILL.md present at session startup must appear in the
 * `/` slash-command autocomplete.
 *
 * Setup: cwd is set to the sandbox dir so V2 picks the BuiltIn
 * `kiro_default` agent (no workspace agent overrides), whose resource
 * list includes `skill://.kiro/skills/*\/SKILL.md`. The skill file
 * is written via `prelaunchFiles` BEFORE spawn so V2's one-shot
 * `advertise_commands_and_prompts` finds it during session init.
 */

import { afterEach, describe, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { E2ETestCase } from './E2ETestCase';

const SKILL_NAME = 'e2e-test-skill';
const SKILL_DESCRIPTION = 'An end-to-end test skill';

const SKILL_MD = `---
name: ${SKILL_NAME}
description: ${SKILL_DESCRIPTION}
---

# Test skill body
`;

describe('V2 skill advertisement', () => {
  let testCase: E2ETestCase | null = null;
  let workspaceCwd: string | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    if (workspaceCwd) {
      try {
        fs.rmSync(workspaceCwd, { recursive: true, force: true });
      } catch {
        /* ignore */
      }
      workspaceCwd = null;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  });

  it('includes pre-existing SKILL.md in advertised skills', async () => {
    workspaceCwd = fs.mkdtempSync(
      path.join(os.tmpdir(), 'kiro-e2e-skills-cwd-')
    );
    const skillDir = path.join(workspaceCwd, '.kiro', 'skills', SKILL_NAME);
    fs.mkdirSync(skillDir, { recursive: true });
    fs.writeFileSync(path.join(skillDir, 'SKILL.md'), SKILL_MD);

    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('v2-skills-advertise')
      .withCwd(workspaceCwd)
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);
    await testCase.sleepMs(500);

    await testCase.sendKeys('/');
    await testCase.sleepMs(300);
    for (const ch of SKILL_NAME) {
      await testCase.sendKeys(ch);
      await testCase.sleepMs(30);
    }
    await testCase.sleepMs(500);

    await testCase.waitForText(SKILL_DESCRIPTION, 5000);

    await testCase.sendKeys([0x1b]);
    await testCase.sleepMs(200);
    await testCase.pressCtrlCTwice();
    await testCase.expectExit();
  }, 60000);
});
