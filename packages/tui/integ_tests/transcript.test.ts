/**
 * Integration tests for /transcript command.
 *
 * Tests cover:
 * - Shows error alert when no conversation messages exist
 * - Opens pager with conversation content when messages exist
 * - Opens pager at the bottom of the file (+G)
 * - Opens pager with plaintext format (/transcript --plain)
 * - Saves transcript to file (/transcript save)
 * - Saves plaintext transcript (/transcript save --plain)
 * - Saves JSON transcript (/transcript save --json)
 * - Saves to a specific path (/transcript save <path>)
 * - Expands ~ in save path
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { existsSync, readFileSync, mkdtempSync, unlinkSync } from 'fs';
import { join } from 'path';
import { tmpdir, homedir } from 'os';
import { TestCase } from '../src/test-utils/TestCase';

async function typeSlowly(tc: TestCase, text: string) {
  for (const char of text) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
}

describe('/transcript', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('shows error when no messages exist', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-empty')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, '/transcript');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const state = await testCase.getStore();
    expect(state.transientAlert?.message).toBe('No conversation to display');
    expect(state.transientAlert?.status).toBe('error');
  }, 30000);

  it('opens less with conversation markdown and quits with q', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-with-messages')
      .withEnv({ PAGER: 'less', TERM: 'xterm' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // Send a user message so the conversation is non-empty
    await typeSlowly(testCase, 'hello from the test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // Open transcript — less takes over the PTY
    await typeSlowly(testCase, '/transcript');
    await testCase.pressEnter();

    // Wait for less to render the serialized markdown (longer timeout for CI)
    await testCase.waitForVisibleText('## User', 10000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('## User');
    expect(snapshot).toContain('hello from the test');

    // Quit less
    await testCase.sendKeys('q');

    // Should return to the normal TUI prompt
    await testCase.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('opens pager at the bottom of the file', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-bottom')
      .withEnv({ PAGER: 'less', TERM: 'xterm' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'first message');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, '/transcript');
    await testCase.pressEnter();

    // +G means less shows (END) indicator
    await testCase.waitForVisibleText('END', 10000);

    await testCase.sendKeys('q');
    await testCase.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('opens pager with plaintext format', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-plain-pager')
      .withEnv({ PAGER: 'less', TERM: 'xterm' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'hello plain');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, '/transcript --plain');
    await testCase.pressEnter();

    // Plaintext uses "User:" not "## User"
    await testCase.waitForVisibleText('User:', 10000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('User:');
    expect(snapshot).toContain('hello plain');
    expect(snapshot).not.toContain('## User');

    await testCase.sendKeys('q');
    await testCase.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('saves markdown transcript to default path', async () => {
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro-transcript-test-'));
    testCase = await TestCase.builder()
      .withTestName('transcript-save-default')
      .withEnv({ KIRO_CWD: tempDir })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'save test message');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, `/transcript save ${tempDir}/out.md`);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('success');

    const content = readFileSync(`${tempDir}/out.md`, 'utf-8');
    expect(content).toContain('## User');
    expect(content).toContain('save test message');
  }, 30000);

  it('saves plaintext transcript', async () => {
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro-transcript-test-'));
    testCase = await TestCase.builder()
      .withTestName('transcript-save-plain')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'plaintext save test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, `/transcript save ${tempDir}/out.txt --plain`);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('success');

    const content = readFileSync(`${tempDir}/out.txt`, 'utf-8');
    expect(content).toContain('User:');
    expect(content).toContain('plaintext save test');
    expect(content).not.toContain('## User');
  }, 30000);

  it('saves JSON transcript', async () => {
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro-transcript-test-'));
    testCase = await TestCase.builder()
      .withTestName('transcript-save-json')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'json save test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, `/transcript save ${tempDir}/out.json --json`);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('success');

    const content = readFileSync(`${tempDir}/out.json`, 'utf-8');
    const parsed = JSON.parse(content);
    expect(Array.isArray(parsed)).toBe(true);
    expect(parsed[0].role).toBeDefined();
    expect(parsed[0].content).toContain('json save test');
  }, 30000);

  it('opens pager with json format', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-json-pager')
      .withEnv({ PAGER: 'less', TERM: 'xterm' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'json pager test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, '/transcript --json');
    await testCase.pressEnter();

    await testCase.waitForVisibleText('"role"', 10000);

    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('"content"');
    expect(snapshot).toContain('json pager test');

    await testCase.sendKeys('q');
    await testCase.waitForVisibleText('ask a question', 10000);
  }, 30000);

  it('expands ~/ in save path', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-tilde-expand')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'tilde test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const homeTmp = join(homedir(), `.kiro-transcript-test-${Date.now()}.md`);
    const tildeRelative = homeTmp.replace(homedir(), '~');

    await typeSlowly(testCase, `/transcript save ${tildeRelative}`);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('success');
    expect(existsSync(homeTmp)).toBe(true);

    const content = readFileSync(homeTmp, 'utf-8');
    expect(content).toContain('tilde test');

    unlinkSync(homeTmp);
  }, 30000);

  it('saves to path with spaces', async () => {
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro transcript spaces-'));
    testCase = await TestCase.builder()
      .withTestName('transcript-path-spaces')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'spaces test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, `/transcript save ${tempDir}/my file.md`);
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('success');
    expect(existsSync(`${tempDir}/my file.md`)).toBe(true);
  }, 30000);

  it('shows error for unwritable path', async () => {
    testCase = await TestCase.builder()
      .withTestName('transcript-unwritable')
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await typeSlowly(testCase, 'error test');
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    await typeSlowly(testCase, '/transcript save /nonexistent/dir/file.md');
    await testCase.pressEnter();
    await testCase.sleepMs(1000);

    const state = await testCase.getStore();
    expect(state.transientAlert?.status).toBe('error');
    expect(state.transientAlert?.message).toContain('Failed to save');
  }, 30000);
});
