import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import {
  UI_VARIANTS,
  type UiVariant,
  type VariantSurfaces,
} from '../ui-variants.js';
import { AppContainer } from '../AppContainer.js';
import { InlineLayout } from '../InlineLayout.js';
import { LiteLayout } from '../lite/LiteLayout.js';
import {
  LiteApprovalSurface,
  TuiApprovalSurface,
} from '../approval-surface.js';
import { LiteStatusSurface } from '../lite/status-surface.js';
import { TuiStatusSurface } from '../tui-status-surface.js';
import { LiteTaskTray } from '../lite/LiteTaskTray.js';
import { ActivityTray as TuiActivityTray } from '../../ui/activity-tray/index.js';
import type { VariantLayoutProps } from '../variant-layout.js';
import type { StatusSurfaceProps } from '../status-surface.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { ApprovalRequestInfo } from '../../../types/agent-events.js';

class MockTerminal implements Terminal {
  public output = '';

  get columns() {
    return 120;
  }

  get rows() {
    return 24;
  }

  get kittyProtocolActive() {
    return true;
  }

  start(): void {}
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
}

const approval = {
  sessionId: 'session-1',
  toolId: 'test_tool',
  toolCall: {
    toolCallId: 'call-1',
    title: 'test_tool',
    rawInput: {},
  },
  permissionOptions: [],
  trustOptions: [],
  resolve: () => {},
} as ApprovalRequestInfo;

let activeInstance: Instance | null = null;
type VariantSurfaceName = Exclude<keyof VariantSurfaces, 'Layout'>;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

function createVariantStore(variant: UiVariant) {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  store.setState({
    uiMode: variant,
    mode: 'inline',
    sessionId: 'session-1',
    isInitialized: true,
    lite: {
      ...store.getState().lite,
      scrollbackClearToken: -1,
    },
  });
  return store;
}

function surfaceNames(surfaces: VariantSurfaces): VariantSurfaceName[] {
  return Object.keys(surfaces).filter(
    (name): name is VariantSurfaceName => name !== 'Layout'
  );
}

function createSurfaceStubs(
  names: VariantSurfaceName[],
  onRender: (name: VariantSurfaceName) => void = () => {}
): VariantLayoutProps {
  return Object.fromEntries(
    names.map((name) => [
      name,
      () => {
        onRender(name);
        return null;
      },
    ])
  ) as unknown as VariantLayoutProps;
}

describe('UI variant layout rendering', () => {
  it('maps and routes every variant through AppContainer', async () => {
    expect(UI_VARIANTS).toEqual({
      tui: {
        Layout: InlineLayout,
        ApprovalPrompt: TuiApprovalSurface,
        StatusLine: TuiStatusSurface,
        ActivityTray: TuiActivityTray,
      },
      lite: {
        Layout: LiteLayout,
        ApprovalPrompt: LiteApprovalSurface,
        StatusLine: LiteStatusSurface,
        ActivityTray: LiteTaskTray,
      },
    } satisfies Record<UiVariant, VariantSurfaces>);

    for (const variant of Object.keys(UI_VARIANTS) as UiVariant[]) {
      const surfaces = UI_VARIANTS[variant] as VariantSurfaces;
      const originalSurfaces = { ...surfaces };
      const injectedSurfaces = createSurfaceStubs(surfaceNames(surfaces));
      const received: { props?: VariantLayoutProps } = {};
      const Layout: VariantSurfaces['Layout'] = (props) => {
        received.props = props;
        return null;
      };
      const store = createVariantStore(variant);

      try {
        Object.assign(surfaces, { Layout, ...injectedSurfaces });
        activeInstance = render(
          React.createElement(
            AppStoreContext.Provider,
            { value: store },
            React.createElement(AppContainer)
          ),
          { terminal: new MockTerminal(), exitOnCtrlC: false }
        );
        await flush();
        expect(received.props).toEqual(injectedSurfaces);
      } finally {
        activeInstance?.unmount();
        activeInstance = null;
        Object.assign(surfaces, originalSurfaces);
      }
    }
  });

  it('renders every injected surface from each registered layout', async () => {
    for (const [variant, { Layout }] of Object.entries(UI_VARIANTS) as Array<
      [UiVariant, VariantSurfaces]
    >) {
      const names = surfaceNames(UI_VARIANTS[variant]);
      const rendered = new Set<VariantSurfaceName>();
      const injectedSurfaces = createSurfaceStubs(names, (name) =>
        rendered.add(name)
      );
      const store = createVariantStore(variant);

      activeInstance = render(
        React.createElement(
          AppStoreContext.Provider,
          { value: store },
          React.createElement(Layout, injectedSurfaces)
        ),
        { terminal: new MockTerminal(), exitOnCtrlC: false }
      );
      await flush();

      store.setState({ pendingApproval: approval });
      await flush();
      expect(rendered).toEqual(new Set(names));

      activeInstance.unmount();
      activeInstance = null;
    }
  });

  it('forwards cloud location state through each layout status surface', async () => {
    for (const [variant, { Layout }] of Object.entries(UI_VARIANTS) as Array<
      [UiVariant, VariantSurfaces]
    >) {
      const injectedSurfaces = createSurfaceStubs(
        surfaceNames(UI_VARIANTS[variant])
      );
      let statusProps: StatusSurfaceProps | undefined;
      injectedSurfaces.StatusLine = (props) => {
        statusProps = props;
        return null;
      };
      const store = createVariantStore(variant);
      store.setState({
        cloudSessionActive: true,
        cloudRepo: 'acme/cloud-app',
        cloudBranch: 'cloud-main',
        cloudExtraRepos: 2,
        bootProgress: new Map([
          [
            'session_create',
            {
              label: 'Creating session',
              status: 'ready',
              startTime: 0,
            },
          ],
        ]),
      });

      activeInstance = render(
        React.createElement(
          AppStoreContext.Provider,
          { value: store },
          React.createElement(Layout, injectedSurfaces)
        ),
        { terminal: new MockTerminal(), exitOnCtrlC: false }
      );
      await flush();

      expect(statusProps).toMatchObject({
        cloudSessionActive: true,
        cloudRepo: 'acme/cloud-app',
        cloudBranch: 'cloud-main',
        cloudExtraRepos: 2,
      });

      activeInstance.unmount();
      activeInstance = null;
    }
  });

  it('renders cloud location instead of local location in both variants', async () => {
    const props: StatusSurfaceProps = {
      agentName: 'kiro',
      modelName: 'model',
      effort: null,
      contextUsagePercent: 10,
      workspacePath: '/local-only-workspace',
      gitBranch: 'local-only-branch',
      goalStatus: null,
      cloudSessionActive: true,
      cloudRepo: 'acme/cloud-app',
      cloudBranch: 'cloud-main',
      cloudExtraRepos: 2,
    };

    for (const StatusLine of [TuiStatusSurface, LiteStatusSurface]) {
      const terminal = new MockTerminal();
      activeInstance = render(React.createElement(StatusLine, props), {
        terminal,
        exitOnCtrlC: false,
      });
      await flush();

      const output = stripAnsi(terminal.output);
      expect(output).toContain('Cloud');
      expect(output).toContain('~/kiro/cloud-app');
      expect(output).toContain('cloud-main');
      expect(output).toContain('(+2 others)');
      expect(output).not.toContain('/local-only-workspace');
      expect(output).not.toContain('local-only-branch');

      activeInstance.unmount();
      activeInstance = null;
    }

    const dimmedTerminal = new MockTerminal();
    activeInstance = render(
      React.createElement(TuiStatusSurface, { ...props, dimmed: true }),
      {
        terminal: dimmedTerminal,
        exitOnCtrlC: false,
      }
    );
    await flush();

    const dimmedOutput = stripAnsi(dimmedTerminal.output);
    expect(dimmedOutput).toContain('Cloud');
    expect(dimmedOutput).toContain('~/kiro/cloud-app');
    expect(dimmedOutput).toContain('cloud-main');
    expect(dimmedOutput).toContain('(+2 others)');
    expect(dimmedOutput).not.toContain('/local-only-workspace');
    expect(dimmedOutput).not.toContain('local-only-branch');
  });

  it('preserves local location in both variants outside cloud sessions', async () => {
    const props: StatusSurfaceProps = {
      agentName: 'kiro',
      modelName: 'model',
      effort: null,
      contextUsagePercent: 10,
      workspacePath: '/local-workspace',
      gitBranch: 'local-branch',
      goalStatus: null,
      cloudSessionActive: false,
      cloudRepo: 'acme/cloud-app',
      cloudBranch: 'cloud-main',
      cloudExtraRepos: 2,
    };

    for (const StatusLine of [TuiStatusSurface, LiteStatusSurface]) {
      const terminal = new MockTerminal();
      activeInstance = render(React.createElement(StatusLine, props), {
        terminal,
        exitOnCtrlC: false,
      });
      await flush();

      const output = stripAnsi(terminal.output);
      expect(output).toContain('/local-workspace');
      expect(output).toContain('local-branch');
      expect(output).not.toContain('Cloud');
      expect(output).not.toContain('~/kiro/cloud-app');
      expect(output).not.toContain('cloud-main');

      activeInstance.unmount();
      activeInstance = null;
    }
  });
});
