import { describe, test, expect, afterEach } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import { createAppStore, AppStoreContext, type AppState } from './app-store.js';
import { Kiro } from '../kiro.js';
import {
  useNotificationState,
  useNotificationActions,
  useCommandState,
  useCommandActions,
  useProcessingState,
  useApprovalState,
  useConversationState,
  useUIState,
  useUIActions,
  useContextState,
  useKiroClient,
  useStreamingBuffer,
  useInputActions,
  useFileAttachmentState,
  useFileAttachmentActions,
  useImageAttachmentState,
  useImageAttachmentActions,
  useQueueState,
  useQueueActions,
  useTaskState,
  useTaskActions,
} from './selectors.js';

// ---------------------------------------------------------------------------
// MockTerminal -- minimal Terminal implementation for headless rendering
// ---------------------------------------------------------------------------
class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;

  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }

  start(onInput: (data: string) => void, _onResize: () => void): void {
    this._onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(_data: string): void {}
  moveBy(_lines: number): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(_title: string): void {}
}

// ---------------------------------------------------------------------------
// renderSelectorHook -- renders a hook inside an AppStoreContext provider
// ---------------------------------------------------------------------------
let activeInstance: Instance | null = null;

afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
});

async function renderSelectorHook<T>(
  hook: () => T,
  storeOverrides?: Partial<AppState>
): Promise<T> {
  const { captured } = await renderSelectorHookWithStore(
    hook,
    undefined,
    storeOverrides
  );
  return captured;
}

async function renderSelectorHookWithStore<T>(
  hook: () => T,
  existingStore?: ReturnType<typeof createAppStore>,
  storeOverrides?: Partial<AppState>
): Promise<{ captured: T; store: ReturnType<typeof createAppStore> }> {
  const store = existingStore ?? createAppStore({ kiro: new Kiro() });
  if (storeOverrides) {
    store.setState(storeOverrides as any);
  }

  let captured: T | undefined;

  function TestComponent() {
    captured = hook();
    return null;
  }

  const Wrapper = () => (
    <AppStoreContext.Provider value={store}>
      <TestComponent />
    </AppStoreContext.Provider>
  );

  const instance = render(<Wrapper />, {
    terminal: new MockTerminal(),
    exitOnCtrlC: false,
  });
  activeInstance = instance;

  await new Promise((resolve) => setTimeout(resolve, 50));

  instance.unmount();
  activeInstance = null;

  return { captured: captured as T, store };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('Notification selectors', () => {
  test('useNotificationState returns expected keys', async () => {
    const result = await renderSelectorHook(useNotificationState);
    expect(result).toHaveProperty('transientAlert');
    expect(result).toHaveProperty('loadingMessage');
    expect(result).toHaveProperty('agentError');
    expect(result).toHaveProperty('agentErrorGuidance');
    expect(result).toHaveProperty('initErrors');
    expect(result).toHaveProperty('pendingOAuthServers');
  });

  test('useNotificationState reflects store overrides', async () => {
    const result = await renderSelectorHook(useNotificationState, {
      agentError: 'test error',
      loadingMessage: 'loading...',
    });
    expect(result.agentError).toBe('test error');
    expect(result.loadingMessage).toBe('loading...');
  });

  test('useNotificationActions returns functions', async () => {
    const result = await renderSelectorHook(useNotificationActions);
    expect(typeof result.showTransientAlert).toBe('function');
    expect(typeof result.dismissTransientAlert).toBe('function');
    expect(typeof result.setAgentError).toBe('function');
    expect(typeof result.setLoadingMessage).toBe('function');
  });

  test('useNotificationState default values match initial store state', async () => {
    const result = await renderSelectorHook(useNotificationState);
    expect(result.transientAlert).toBeNull();
    expect(result.loadingMessage).toBeNull();
    expect(result.agentError).toBeNull();
    expect(result.agentErrorGuidance).toBeNull();
    expect(result.initErrors).toEqual([]);
  });

  test('useNotificationActions.setAgentError updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { captured: actions } = await renderSelectorHookWithStore(
      useNotificationActions,
      store
    );
    actions.setAgentError('network timeout');
    expect(store.getState().agentError).toBe('network timeout');
  });

  test('useNotificationActions.setLoadingMessage updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { captured: actions } = await renderSelectorHookWithStore(
      useNotificationActions,
      store
    );
    actions.setLoadingMessage('Connecting...');
    expect(store.getState().loadingMessage).toBe('Connecting...');
    actions.setLoadingMessage(null);
    expect(store.getState().loadingMessage).toBeNull();
  });
});

describe('Command selectors', () => {
  test('useCommandState returns expected keys', async () => {
    const result = await renderSelectorHook(useCommandState);
    expect(result).toHaveProperty('slashCommands');
    expect(result).toHaveProperty('activeCommand');
    expect(result).toHaveProperty('commandInputValue');
    expect(result).toHaveProperty('activeTrigger');
    expect(result).toHaveProperty('filePickerHasResults');
    expect(result).toHaveProperty('promptHint');
    expect(result).toHaveProperty('commandShadowText');
  });

  test('useCommandState reflects store overrides', async () => {
    const result = await renderSelectorHook(useCommandState, {
      commandInputValue: 'test-cmd',
    });
    expect(result.commandInputValue).toBe('test-cmd');
  });

  test('useCommandActions returns functions', async () => {
    const result = await renderSelectorHook(useCommandActions);
    expect(typeof result.setSlashCommands).toBe('function');
    expect(typeof result.setActiveCommand).toBe('function');
    expect(typeof result.setCommandInput).toBe('function');
    expect(typeof result.setActiveTrigger).toBe('function');
    expect(typeof result.setFilePickerHasResults).toBe('function');
    expect(typeof result.setPromptHint).toBe('function');
    expect(typeof result.setCommandShadowText).toBe('function');
    expect(typeof result.clearCommandInput).toBe('function');
    expect(typeof result.executeCommandWithArg).toBe('function');
  });
});

describe('Processing selectors', () => {
  test('useProcessingState returns expected keys', async () => {
    const result = await renderSelectorHook(useProcessingState);
    expect(result).toHaveProperty('isProcessing');
    expect(result).toHaveProperty('isCompacting');
    expect(result).toHaveProperty('isShellEscape');
    expect(result).toHaveProperty('pendingApproval');
    expect(result).toHaveProperty('cancelMessage');
    expect(result).toHaveProperty('noInteractive');
  });

  test('useProcessingState reflects store overrides', async () => {
    const result = await renderSelectorHook(useProcessingState, {
      isProcessing: true,
    });
    expect(result.isProcessing).toBe(true);
  });

  test('useProcessingState default values match initial store state', async () => {
    const result = await renderSelectorHook(useProcessingState);
    expect(result.isProcessing).toBe(false);
    expect(result.isCompacting).toBe(false);
    expect(result.isShellEscape).toBe(false);
    expect(result.pendingApproval).toBeNull();
    expect(result.noInteractive).toBe(false);
  });
});

describe('Approval selectors', () => {
  test('useApprovalState returns expected keys', async () => {
    const result = await renderSelectorHook(useApprovalState);
    expect(result).toHaveProperty('pendingApproval');
    expect(result).toHaveProperty('approvalMode');
    expect(result).toHaveProperty('respondToApproval');
    expect(result).toHaveProperty('cancelApproval');
    expect(result).toHaveProperty('setApprovalMode');
    expect(result).toHaveProperty('sessionId');
    expect(result).toHaveProperty('sessions');
  });
});

describe('Conversation selectors', () => {
  test('useConversationState returns expected keys', async () => {
    const result = await renderSelectorHook(useConversationState);
    expect(result).toHaveProperty('messages');
    expect(result).toHaveProperty('isProcessing');
    expect(result).toHaveProperty('settings');
  });

  test('useConversationState reflects store overrides', async () => {
    const result = await renderSelectorHook(useConversationState, {
      isProcessing: true,
    });
    expect(result.isProcessing).toBe(true);
    expect(Array.isArray(result.messages)).toBe(true);
  });

  test('useConversationState default values match initial store state', async () => {
    const result = await renderSelectorHook(useConversationState);
    expect(result.messages).toEqual([]);
    expect(result.isProcessing).toBe(false);
  });
});

describe('UI selectors', () => {
  test('useUIState returns expected keys', async () => {
    const result = await renderSelectorHook(useUIState);
    expect(result).toHaveProperty('mode');
    expect(result).toHaveProperty('exitSequence');
    expect(result).toHaveProperty('toolOutputsExpanded');
    expect(result).toHaveProperty('hasExpandableToolOutputs');
    expect(result).toHaveProperty('showContextBreakdown');
    expect(result).toHaveProperty('contextBreakdown');
    expect(result).toHaveProperty('showTuiPanel');
    expect(result).toHaveProperty('showHelpPanel');
    expect(result).toHaveProperty('helpCommands');
    expect(result).toHaveProperty('showUsagePanel');
    expect(result).toHaveProperty('usageData');
    expect(result).toHaveProperty('showMcpPanel');
    expect(result).toHaveProperty('mcpServers');
    expect(result).toHaveProperty('mcpRegistryServers');
    expect(result).toHaveProperty('mcpMode');
    expect(result).toHaveProperty('showToolsPanel');
    expect(result).toHaveProperty('toolsList');
    expect(result).toHaveProperty('showHooksPanel');
    expect(result).toHaveProperty('hooksList');
    expect(result).toHaveProperty('showKnowledgePanel');
    expect(result).toHaveProperty('knowledgeEntries');
    expect(result).toHaveProperty('knowledgeStatus');
    expect(result).toHaveProperty('showCodePanel');
    expect(result).toHaveProperty('codeData');
  });

  test('useUIActions returns functions', async () => {
    const result = await renderSelectorHook(useUIActions);
    expect(typeof result.setMode).toBe('function');
    expect(typeof result.incrementExitSequence).toBe('function');
    expect(typeof result.resetExitSequence).toBe('function');
    expect(typeof result.toggleToolOutputsExpanded).toBe('function');
    expect(typeof result.setHasExpandableToolOutputs).toBe('function');
    expect(typeof result.setShowContextBreakdown).toBe('function');
    expect(typeof result.setShowHelpPanel).toBe('function');
    expect(typeof result.setShowTuiPanel).toBe('function');
    expect(typeof result.setShowUsagePanel).toBe('function');
    expect(typeof result.setShowMcpPanel).toBe('function');
    expect(typeof result.setShowToolsPanel).toBe('function');
    expect(typeof result.setShowHooksPanel).toBe('function');
    expect(typeof result.setShowKnowledgePanel).toBe('function');
    expect(typeof result.setShowCodePanel).toBe('function');
  });

  test('useUIState default values match initial store state', async () => {
    const result = await renderSelectorHook(useUIState);
    expect(result.mode).toBe('inline');
    expect(result.exitSequence).toBe(0);
    expect(result.toolOutputsExpanded).toBe(false);
    expect(result.hasExpandableToolOutputs).toBe(false);
    expect(result.showContextBreakdown).toBe(false);
    expect(result.showHelpPanel).toBe(false);
    expect(result.showMcpPanel).toBe(false);
  });

  test('useUIActions.setMode updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { captured: actions } = await renderSelectorHookWithStore(
      useUIActions,
      store
    );
    actions.setMode('expanded');
    expect(store.getState().mode).toBe('expanded');
  });
});

describe('Context selectors', () => {
  test('useContextState returns expected keys', async () => {
    const result = await renderSelectorHook(useContextState);
    expect(result).toHaveProperty('sessionId');
    expect(result).toHaveProperty('contextUsagePercent');
    expect(result).toHaveProperty('lastTurnTokens');
    expect(result).toHaveProperty('currentModel');
    expect(result).toHaveProperty('currentAgent');
    expect(result).toHaveProperty('previousAgentName');
    expect(result).toHaveProperty('codeIntelligenceActive');
  });

  test('useContextState reflects store overrides', async () => {
    const result = await renderSelectorHook(useContextState, {
      contextUsagePercent: 42,
      currentModel: { id: 'test-model', name: 'Test Model' },
    });
    expect(result.contextUsagePercent).toBe(42);
    expect(result.currentModel).toEqual({
      id: 'test-model',
      name: 'Test Model',
    });
  });
});

describe('Kiro client selector', () => {
  test('useKiroClient returns kiro instance', async () => {
    const result = await renderSelectorHook(useKiroClient);
    expect(result).toHaveProperty('kiro');
    expect(result.kiro).toBeInstanceOf(Kiro);
  });
});

describe('Streaming buffer selector', () => {
  test('useStreamingBuffer returns expected keys', async () => {
    const result = await renderSelectorHook(useStreamingBuffer);
    expect(result).toHaveProperty('startBuffering');
    expect(result).toHaveProperty('stopBuffering');
  });
});

describe('Input actions selector', () => {
  test('useInputActions returns functions', async () => {
    const result = await renderSelectorHook(useInputActions);
    expect(typeof result.handleUserInput).toBe('function');
    expect(typeof result.clearInput).toBe('function');
    expect(typeof result.insert).toBe('function');
    expect(typeof result.newline).toBe('function');
    expect(typeof result.backspace).toBe('function');
    expect(typeof result.moveCursor).toBe('function');
    expect(typeof result.setViewport).toBe('function');
    expect(typeof result.navigateHistory).toBe('function');
  });
});

describe('File attachment selectors', () => {
  test('useFileAttachmentState returns expected keys', async () => {
    const result = await renderSelectorHook(useFileAttachmentState);
    expect(result).toHaveProperty('attachedFiles');
    expect(result).toHaveProperty('pendingFileAttachment');
  });

  test('useFileAttachmentState reflects store overrides', async () => {
    const result = await renderSelectorHook(useFileAttachmentState, {
      attachedFiles: ['/path/to/file.ts'],
    });
    expect(result.attachedFiles).toEqual(['/path/to/file.ts']);
  });

  test('useFileAttachmentActions returns functions', async () => {
    const result = await renderSelectorHook(useFileAttachmentActions);
    expect(typeof result.attachFile).toBe('function');
    expect(typeof result.removeAttachedFile).toBe('function');
    expect(typeof result.clearAttachedFiles).toBe('function');
    expect(typeof result.setPendingFileAttachment).toBe('function');
    expect(typeof result.consumePendingFileAttachment).toBe('function');
  });

  test('useFileAttachmentState default values match initial store state', async () => {
    const result = await renderSelectorHook(useFileAttachmentState);
    expect(result.attachedFiles).toEqual([]);
    expect(result.pendingFileAttachment).toBeNull();
  });

  test('useFileAttachmentActions.attachFile updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const { captured: actions } = await renderSelectorHookWithStore(
      useFileAttachmentActions,
      store
    );
    actions.attachFile('/path/to/file.ts');
    expect(store.getState().attachedFiles).toEqual(['/path/to/file.ts']);
    actions.attachFile('/path/to/other.ts');
    expect(store.getState().attachedFiles).toEqual([
      '/path/to/file.ts',
      '/path/to/other.ts',
    ]);
  });

  test('useFileAttachmentActions.removeAttachedFile updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.setState({ attachedFiles: ['/a.ts', '/b.ts', '/c.ts'] });
    const { captured: actions } = await renderSelectorHookWithStore(
      useFileAttachmentActions,
      store
    );
    actions.removeAttachedFile('/b.ts');
    expect(store.getState().attachedFiles).toEqual(['/a.ts', '/c.ts']);
  });
});

describe('Image attachment selectors', () => {
  test('useImageAttachmentState returns expected keys', async () => {
    const result = await renderSelectorHook(useImageAttachmentState);
    expect(result).toHaveProperty('pendingImages');
  });

  test('useImageAttachmentActions returns functions', async () => {
    const result = await renderSelectorHook(useImageAttachmentActions);
    expect(typeof result.addPendingImage).toBe('function');
    expect(typeof result.removePendingImage).toBe('function');
    expect(typeof result.clearPendingImages).toBe('function');
  });
});

describe('Queue selectors', () => {
  test('useQueueState returns expected keys', async () => {
    const result = await renderSelectorHook(useQueueState);
    expect(result).toHaveProperty('queuedMessages');
    expect(result).toHaveProperty('editingQueueIndex');
  });

  test('useQueueState reflects store overrides', async () => {
    const result = await renderSelectorHook(useQueueState, {
      queuedMessages: ['msg1', 'msg2'],
    });
    expect(result.queuedMessages).toEqual(['msg1', 'msg2']);
  });

  test('useQueueActions returns functions', async () => {
    const result = await renderSelectorHook(useQueueActions);
    expect(typeof result.removeQueuedMessage).toBe('function');
    expect(typeof result.replaceQueuedMessage).toBe('function');
    expect(typeof result.startEditingQueue).toBe('function');
    expect(typeof result.cancelEditingQueue).toBe('function');
  });

  test('useQueueState default values match initial store state', async () => {
    const result = await renderSelectorHook(useQueueState);
    expect(result.queuedMessages).toEqual([]);
    expect(result.editingQueueIndex).toBeNull();
  });

  test('useQueueActions.removeQueuedMessage updates store state', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.setState({ queuedMessages: ['msg1', 'msg2', 'msg3'] });
    const { captured: actions } = await renderSelectorHookWithStore(
      useQueueActions,
      store
    );
    actions.removeQueuedMessage(1);
    expect(store.getState().queuedMessages).toEqual(['msg1', 'msg3']);
  });
});

describe('Task selectors', () => {
  test('useTaskState returns expected keys', async () => {
    const result = await renderSelectorHook(useTaskState);
    expect(result).toHaveProperty('tasks');
    expect(result).toHaveProperty('activityTrayExpanded');
  });

  test('useTaskState reflects store overrides', async () => {
    const result = await renderSelectorHook(useTaskState, {
      activityTrayExpanded: true,
      tasks: [{ id: '1', subject: 'test', status: 'pending' as const }],
    });
    expect(result.activityTrayExpanded).toBe(true);
    expect(result.tasks).toHaveLength(1);
  });

  test('useTaskActions returns a function', async () => {
    const result = await renderSelectorHook(useTaskActions);
    expect(typeof result).toBe('function');
  });
});
