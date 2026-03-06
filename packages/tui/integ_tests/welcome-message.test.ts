import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';

describe('Agent Welcome Message', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('displays welcome message on startup when agent has one configured', async () => {
    testCase = await TestCase.builder()
      .withTestName('welcome-message-startup')
      .withTimeout(15000)
      .withEnv({
        KIRO_MOCK_WELCOME_MESSAGE: '👋 Welcome to the test agent!',
        KIRO_MOCK_AGENT_NAME: 'test-agent',
      })
      .launch();

    // Wait for TUI to render
    await testCase.waitForVisibleText('ask a question');

    // Verify welcome message appears in the store
    const store = await testCase.getStore();
    const welcomeMsg = store.messages.find(
      (m) => m.role === 'model' && m.content.includes('👋 Welcome to the test agent!')
    );
    expect(welcomeMsg).toBeDefined();
    expect(store.currentAgent?.name).toBe('test-agent');

    // Exit
    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('does not display welcome message when agent has none', async () => {
    testCase = await TestCase.builder()
      .withTestName('welcome-message-none')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    const store = await testCase.getStore();
    const modelMessages = store.messages.filter((m) => m.role === 'model');
    expect(modelMessages).toHaveLength(0);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
