import React, { useState } from 'react';
import { Box } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
  type HookInfo,
} from '../../stores/app-store.js';
import type {
  StorybookAssertions,
  StorybookExperience,
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { SettingsPanel } from '../ui/SettingsPanel.js';
import { DisplaySettingsPanel } from '../ui/DisplaySettingsPanel.js';
import { ThemePanel } from '../ui/ThemePanel.js';
import { StatusLineSettingsPanel } from '../ui/StatusLineSettingsPanel.js';
import { KeybindingsPanel } from '../ui/KeybindingsPanel.js';
import { HelpPanel } from '../ui/HelpPanel.js';
import { HooksPanel } from '../ui/HooksPanel.js';
import { ConfigPanel } from '../ui/ConfigPanel.js';
import type {
  ConfigCategoryId,
  ConfigSnapshot,
} from '../ui/config-panel-model.js';
import { VerbosityPreview } from '../ui/menu/VerbosityPreview.js';
import { TuiVerbosityPreview } from '../ui/menu/TuiVerbosityPreview.js';
import {
  VerbosityTruncationEditor,
  type TruncationEditorField,
} from '../ui/menu/VerbosityTruncationEditor.js';

type PanelKind =
  | 'settings-tui'
  | 'settings-lite'
  | 'display-tui'
  | 'display-lite'
  | 'status-line-tui'
  | 'status-line-lite'
  | 'theme'
  | 'keybindings'
  | 'verbosity-tui'
  | 'verbosity-lite'
  | 'truncation-tui'
  | 'truncation-lite'
  | 'help'
  | 'hooks'
  | 'hooks-empty'
  | 'config'
  | 'config-steering'
  | 'config-skills'
  | 'config-powers';

interface SettingsPanelsStoryProps {
  panel: PanelKind;
}

const viewport = { columns: 150, rows: 46 };
const environment = {
  KIRO_HOME: '/tmp/kiro-visual-settings-panels',
  KIRO_LITE_ROLLOUT_ENABLED: '1',
  KIRO_ENABLED_FEATURES: '["cloud_config","workflows"]',
};

const hooks: HookInfo[] = [
  {
    name: 'release-guard',
    trigger: 'preToolUse',
    command: 'test',
    matcher: 'write|shell',
    configSource: 'cloud',
  },
  {
    name: 'format-after-write',
    trigger: 'postToolUse',
    command: 'format',
    matcher: 'write',
    configSource: 'local',
  },
  {
    name: 'audit-prompt',
    trigger: 'userPromptSubmit',
    command: 'audit',
    configSource: 'local',
  },
];

const configSnapshot: ConfigSnapshot = {
  cloudSession: false,
  sourcesReported: true,
  agents: [
    {
      id: 'builder',
      name: 'Builder',
      description: 'General implementation agent',
      configSource: 'local',
    },
    {
      id: 'reviewer',
      name: 'Reviewer',
      description: 'Adversarial review agent',
      configSource: 'cloud',
    },
  ],
  mcpServers: [
    {
      name: 'github',
      status: 'running',
      toolCount: 12,
      source: 'local',
    },
    {
      name: 'documentation',
      status: 'disabled',
      toolCount: 4,
      source: 'cloud',
    },
  ],
  powers: [
    {
      name: 'release-certification',
      displayName: 'Release certification',
      description: 'Runs release-candidate verification',
      configSource: 'cloud',
    },
    {
      name: 'dependency-audit',
      displayName: 'Dependency audit',
      description: 'Checks dependency health',
      configSource: 'local',
    },
  ],
  steering: [],
  steeringDocs: [
    {
      name: 'architecture.md',
      scope: 'workspace',
      inclusion: 'always',
      configSource: 'local',
    },
    {
      name: 'coding-standards.md',
      scope: 'global',
      inclusion: 'fileMatch',
      configSource: 'cloud',
    },
  ],
  skills: [
    {
      name: 'release-review',
      description: 'Review release evidence',
      source: { kind: 'workspace' },
      configSource: 'cloud',
    },
    {
      name: 'test-authoring',
      description: 'Build deterministic test scenarios',
      source: { kind: 'global' },
      configSource: 'local',
    },
  ],
  hooks,
  diagnostics: [
    {
      severity: 'warning',
      message: 'Cloud settings refresh is pending',
    },
  ],
};

const helpCommands = [
  {
    name: '/chat',
    description: 'Save, load, and inspect conversations',
    usage: '/chat <subcommand>',
    subcommands: ['save', 'load', 'resume'],
  },
  {
    name: '/workflow',
    description: 'Manage workflow runs and history',
    usage: '/workflow <subcommand>',
    subcommands: ['list', 'resume', 'stop'],
  },
  {
    name: '/settings',
    description: 'Configure display and terminal preferences',
    usage: '/settings',
  },
  {
    name: '/hooks',
    description: 'Inspect configured lifecycle hooks',
    usage: '/hooks',
  },
  {
    name: '/config',
    description: 'Inspect effective local and cloud configuration',
    usage: '/config <category>',
  },
];

function createStoryStore(panel: PanelKind): AppStoreApi {
  const uiMode = panel.endsWith('-lite') ? 'lite' : 'tui';
  const agentEngine =
    panel.startsWith('config') || panel.startsWith('settings') ? 'kas' : 'v2';
  return createAppStore({
    kiro: new Kiro(),
    agentEngine,
    uiMode,
  });
}

function ConfigStory({
  initialCategory,
}: {
  initialCategory?: ConfigCategoryId;
}): React.ReactElement {
  return (
    <ConfigPanel
      snapshot={configSnapshot}
      initialCategory={initialCategory}
      onClose={() => undefined}
      onOpenMcp={() => undefined}
      onOpenHooks={() => undefined}
      onOpenAgent={() => undefined}
    />
  );
}

function SettingsPanelsStory({
  panel,
}: SettingsPanelsStoryProps): React.ReactElement {
  const [store] = useState(() => createStoryStore(panel));

  let content: React.ReactElement;
  switch (panel) {
    case 'settings-tui':
    case 'settings-lite':
      content = <SettingsPanel onClose={() => undefined} />;
      break;
    case 'display-tui':
      content = (
        <DisplaySettingsPanel surface="tui" onClose={() => undefined} />
      );
      break;
    case 'display-lite':
      content = (
        <DisplaySettingsPanel surface="lite" onClose={() => undefined} />
      );
      break;
    case 'status-line-tui':
      content = (
        <StatusLineSettingsPanel surface="tui" onClose={() => undefined} />
      );
      break;
    case 'status-line-lite':
      content = (
        <StatusLineSettingsPanel surface="lite" onClose={() => undefined} />
      );
      break;
    case 'theme':
      content = <ThemePanel onClose={() => undefined} />;
      break;
    case 'keybindings':
      content = <KeybindingsPanel onClose={() => undefined} />;
      break;
    case 'verbosity-tui':
      content = <TuiVerbosityPreview which="top" />;
      break;
    case 'verbosity-lite':
      content = <VerbosityPreview which="top" />;
      break;
    case 'truncation-tui':
    case 'truncation-lite':
      content = (
        <VerbosityTruncationEditor
          which={'outputLines' satisfies TruncationEditorField}
          onCommit={() => undefined}
          onCancel={() => undefined}
        />
      );
      break;
    case 'help':
      content = <HelpPanel commands={helpCommands} onClose={() => undefined} />;
      break;
    case 'hooks':
      content = <HooksPanel hooks={hooks} onClose={() => undefined} />;
      break;
    case 'hooks-empty':
      content = <HooksPanel hooks={[]} onClose={() => undefined} />;
      break;
    case 'config':
      content = <ConfigStory />;
      break;
    case 'config-steering':
      content = <ConfigStory initialCategory="steering" />;
      break;
    case 'config-skills':
      content = <ConfigStory initialCategory="skills" />;
      break;
    case 'config-powers':
      content = <ConfigStory initialCategory="powers" />;
      break;
  }

  return (
    <AppStoreContext.Provider value={store}>
      <Box flexDirection="column">{content}</Box>
    </AppStoreContext.Provider>
  );
}

type Captures = NonNullable<
  NonNullable<StorybookParameters['certification']>['captures']
>;

interface CertificationOptions {
  experience: StorybookExperience;
  readyText: string;
  assertions: StorybookAssertions;
  coversVisualStates?: readonly string[];
  captures?: Captures;
}

function certification({
  experience,
  readyText,
  assertions,
  coversVisualStates,
  captures,
}: CertificationOptions): StorybookParameters {
  return {
    layout: 'fullscreen',
    experience,
    capturesKeyboard: captures !== undefined,
    ...(captures ? {} : { coversVisualStates }),
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      environment,
      assertions: {
        visible: assertions.visible,
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
        styled: assertions.styled,
      },
      ...(captures ? { captures } : {}),
    },
  };
}

const captureSettingsJourney: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('root');
  await press('down');
  await press('down');
  await press('down');
  await press('enter');
  await waitFor('/settings – terminal');
  await capture('terminal');
  await press('down');
  await press('enter');
  await waitFor('/settings – interrupt behaviour');
  await capture('interrupt');
};

const captureThemeJourney: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('picker');
  await press('down');
  await press('down');
  await press('down');
  await press('enter');
  await waitFor('Select a prompt style');
  await capture('custom-prompt');
};

const captureStatusLineTui: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('defaults');
  for (let i = 0; i < 6; i += 1) await press('down');
  await waitFor('Code intelligence');
  await capture('conditional');
  for (let i = 0; i < 8; i += 1) await press('down');
  await waitFor('Reset to defaults');
  await capture('disabled');
};

const captureStatusLineLite: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('defaults');
  for (let i = 0; i < 13; i += 1) await press('down');
  await waitFor('Reset to defaults');
  await capture('disabled');
};

const captureHelpSearch: StorybookPlay = async ({ type, waitFor, capture }) => {
  await capture('catalog');
  await type('workflow');
  await waitFor('Manage workflow runs and history');
  await capture('filtered');
};

const meta = {
  title: 'Compositions/Settings Panels',
  component: SettingsPanelsStory,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'SettingsTuiJourney',
      'SettingsLiteRoot',
      'DisplayTui',
      'DisplayLite',
      'StatusLineTui',
      'StatusLineLite',
      'ThemeJourney',
      'Keybindings',
      'VerbosityTui',
      'VerbosityLite',
      'TruncationEditorTui',
      'TruncationEditorLite',
      'HelpCatalog',
      'HooksConfigured',
      'HooksEmpty',
      'ConfigSummary',
      'ConfigSteering',
      'ConfigSkills',
      'ConfigPowers',
    ],
    visualStates: {
      'settings-root-tui': { label: 'TUI settings category picker' },
      'settings-root-lite': { label: 'Lite settings category picker' },
      'settings-terminal': { label: 'Terminal settings category' },
      'settings-interrupt': { label: 'Default interrupt behavior choices' },
      'display-tui': { label: 'TUI display preferences' },
      'display-lite': { label: 'Lite display preferences' },
      'status-line-tui-defaults': {
        label: 'TUI status-line default segment group',
      },
      'status-line-tui-conditional': {
        label: 'TUI-only conditional status segment',
      },
      'status-line-tui-disabled': {
        label: 'TUI status segments disabled by default',
      },
      'status-line-lite-defaults': {
        label: 'Lite status-line default segment group',
      },
      'status-line-lite-disabled': {
        label: 'Lite status segments disabled by default',
      },
      'theme-picker': { label: 'Theme mode picker and preview frame' },
      'theme-custom-prompt': { label: 'Custom prompt color choices' },
      keybindings: { label: 'Read-only keybinding inventory' },
      'verbosity-tui': {
        label: 'TUI verbosity preview through production components',
      },
      'verbosity-lite': {
        label: 'Lite verbosity preview through the ANSI text renderer',
      },
      'truncation-tui': {
        label: 'TUI output truncation editor chrome',
      },
      'truncation-lite': {
        label: 'Lite output truncation editor with live preview',
      },
      'help-catalog': { label: 'Command help catalog' },
      'help-filtered': { label: 'Help catalog filtered through panel search' },
      'hooks-configured': { label: 'Configured hooks with source facts' },
      'hooks-empty': { label: 'Empty hook inventory' },
      'config-summary': {
        label: 'Effective configuration category summary',
      },
      'config-steering': { label: 'Steering configuration detail' },
      'config-skills': { label: 'Skills configuration detail' },
      'config-powers': { label: 'Powers configuration detail' },
      'settings-panel-handoff': {
        label: 'Settings panel hands off to another overlay',
        description:
          'Requires the layout-owned overlay flags and close/reopen handlers.',
        gapType: 'integration-only',
      },
      'settings-persistence': {
        label: 'Settings changes persist and repaint',
        description:
          'Persistence is asserted by interaction tests; visual certification avoids mutating the operator settings file.',
        gapType: 'integration-only',
      },
      'theme-color-fidelity': {
        label: 'Theme preview foreground and background color fidelity',
      },
      'truncation-tui-output-preview': {
        label: 'TUI truncation preview includes the output being capped',
        description:
          'The production TUI preview currently renders the Shell fixture header without its output rows.',
        gapType: 'product-limitation',
      },
      'hooks-narrow-columns': {
        label: 'Hook command and matcher columns remain readable',
        description:
          'The production panel clips or wraps later columns inside its narrow content width.',
        gapType: 'product-limitation',
      },
      'config-routed-handoff': {
        label: 'Config routes to agent, MCP, and hooks overlays',
        description:
          'Requires backend RPC completion and layout-owned panel transitions.',
        gapType: 'integration-only',
      },
    },
  },
};

export default meta;

export const SettingsTuiJourney = {
  args: { panel: 'settings-tui' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/settings',
    assertions: { visible: ['/settings'] },
    captures: {
      root: {
        label: 'TUI settings category picker',
        coversVisualStates: ['settings-root-tui'],
        assertions: {
          visible: [
            'Display',
            'Verbosity',
            'Theme',
            'Terminal',
            'Keybindings',
            'History',
            'Features',
          ],
          ordered: [
            'Display',
            'Verbosity',
            'Theme',
            'Terminal',
            'Keybindings',
            'History',
            'Features',
          ],
        },
      },
      terminal: {
        label: 'terminal settings category',
        coversVisualStates: ['settings-terminal'],
        assertions: {
          visible: [
            '/settings – terminal',
            'Configure terminal preferences',
            'Newlines',
            'Default Interrupt behaviour',
          ],
          ordered: ['Newlines', 'Default Interrupt behaviour'],
        },
      },
      interrupt: {
        label: 'default interrupt behavior choices',
        coversVisualStates: ['settings-interrupt'],
        assertions: {
          visible: [
            '/settings – interrupt behaviour',
            'Steer',
            'Queue',
            'Inject your message mid-turn',
            'Buffer your message',
          ],
          ordered: ['Steer', 'Queue'],
        },
      },
    },
  }),
  play: captureSettingsJourney,
};

export const SettingsLiteRoot = {
  args: { panel: 'settings-lite' satisfies PanelKind },
  parameters: certification({
    experience: 'lite',
    readyText: '/settings',
    assertions: {
      visible: [
        'Display',
        'Verbosity',
        'Theme',
        'Terminal',
        'Keybindings',
        'History',
      ],
      ordered: [
        'Display',
        'Verbosity',
        'Theme',
        'Terminal',
        'Keybindings',
        'History',
      ],
    },
    coversVisualStates: ['settings-root-lite'],
  }),
};

const displayAssertions: StorybookAssertions = {
  visible: [
    '/settings – display',
    'How would you like Kiro to display output?',
    'Default UI',
    'Animations',
    'ASCII art',
    'Icons',
    'Thinking tips',
    'Terminal title',
    'Status line',
  ],
  ordered: [
    'Default UI',
    'Animations',
    'ASCII art',
    'Icons',
    'Thinking tips',
    'Terminal title',
    'Status line',
  ],
};

export const DisplayTui = {
  args: { panel: 'display-tui' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/settings – display',
    assertions: displayAssertions,
    coversVisualStates: ['display-tui'],
  }),
};

export const DisplayLite = {
  args: { panel: 'display-lite' satisfies PanelKind },
  parameters: certification({
    experience: 'lite',
    readyText: '/settings – display',
    assertions: displayAssertions,
    coversVisualStates: ['display-lite'],
  }),
};

export const StatusLineTui = {
  args: { panel: 'status-line-tui' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/settings – display – status line',
    assertions: {
      visible: ['Which segments should the tui status line show?'],
    },
    captures: {
      defaults: {
        label: 'TUI segments enabled by default',
        coversVisualStates: ['status-line-tui-defaults'],
        assertions: {
          visible: [
            'On by default',
            'Agent',
            'Autonomous',
            'Model',
            'Effort',
            'Context',
          ],
          ordered: ['Agent', 'Autonomous', 'Model', 'Effort', 'Context'],
        },
      },
      conditional: {
        label: 'TUI-only code intelligence segment',
        coversVisualStates: ['status-line-tui-conditional'],
        assertions: {
          visible: ['Code intelligence', 'Only while code intel runs'],
        },
      },
      disabled: {
        label: 'TUI segments disabled by default and reset action',
        coversVisualStates: ['status-line-tui-disabled'],
        assertions: {
          visible: [
            'Off by default',
            'Date',
            'Time',
            'Usage',
            'Credits',
            'Reset to defaults',
          ],
          ordered: ['Date', 'Time', 'Usage', 'Credits', 'Reset to defaults'],
        },
      },
    },
  }),
  play: captureStatusLineTui,
};

export const StatusLineLite = {
  args: { panel: 'status-line-lite' satisfies PanelKind },
  parameters: certification({
    experience: 'lite',
    readyText: '/settings – display – status line',
    assertions: {
      visible: ['Which segments should the lite status line show?'],
      hidden: ['Code intelligence'],
    },
    captures: {
      defaults: {
        label: 'Lite segments enabled by default',
        coversVisualStates: ['status-line-lite-defaults'],
        assertions: {
          visible: [
            'On by default',
            'Agent',
            'Autonomous',
            'Model',
            'Effort',
            'Context',
          ],
          ordered: ['Agent', 'Autonomous', 'Model', 'Effort', 'Context'],
        },
      },
      disabled: {
        label: 'Lite segments disabled by default and reset action',
        coversVisualStates: ['status-line-lite-disabled'],
        assertions: {
          visible: [
            'Off by default',
            'Date',
            'Time',
            'Usage',
            'Credits',
            'Reset to defaults',
          ],
          ordered: ['Date', 'Time', 'Usage', 'Credits', 'Reset to defaults'],
        },
      },
    },
  }),
  play: captureStatusLineLite,
};

export const ThemeJourney = {
  args: { panel: 'theme' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/settings – theme',
    assertions: { visible: ['/settings – theme'] },
    captures: {
      picker: {
        label: 'theme mode picker',
        coversVisualStates: ['theme-picker'],
        assertions: {
          visible: [
            'Select the theme that looks best for your terminal',
            'Auto',
            'Dark theme',
            'Light theme',
            'Custom',
            'preview',
          ],
          ordered: ['Auto', 'Dark theme', 'Light theme', 'Custom', 'preview'],
        },
      },
      'custom-prompt': {
        label: 'custom prompt color choices',
        coversVisualStates: ['theme-custom-prompt', 'theme-color-fidelity'],
        assertions: {
          visible: [
            '/settings – theme – custom',
            'Select a prompt style',
            'Default',
            'Purple',
            'Ocean',
            'Forest',
            'Paper',
            'This is the user input',
            'This is the system response',
          ],
          ordered: ['Default', 'Purple', 'Ocean', 'Forest', 'Paper', 'preview'],
          styled: [
            {
              text: '+  const result = compute(input);',
              foreground: '#80ffb5',
              background: '#2d3a30',
            },
            {
              text: '-  const result = calculate(input);',
              foreground: '#ff8080',
              background: '#3a2d2f',
            },
          ],
        },
      },
    },
  }),
  play: captureThemeJourney,
};

export const Keybindings = {
  args: { panel: 'keybindings' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/settings – keybindings',
    assertions: {
      visible: [
        'Keybindings can be customised in',
        '~/.kiro/settings.json',
        'Cancel streaming',
        "Stop the agent's response",
        'Dismiss overlay',
        'Close panels, menus, overlays',
        'Quit',
        'Exit the CLI',
      ],
      ordered: ['Cancel streaming', 'Dismiss overlay', 'Quit'],
      occurrences: { '[default]': 3 },
    },
    coversVisualStates: ['keybindings'],
  }),
};

const verbosityAssertions: StorybookAssertions = {
  visible: [
    'find the legacy auth middleware',
    'Found four call sites for the legacy middleware',
  ],
  ordered: [
    'find the legacy auth middleware',
    'Found four call sites for the legacy middleware',
  ],
};

export const VerbosityTui = {
  args: { panel: 'verbosity-tui' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: 'find the legacy auth middleware',
    assertions: verbosityAssertions,
    coversVisualStates: ['verbosity-tui'],
  }),
};

export const VerbosityLite = {
  args: { panel: 'verbosity-lite' satisfies PanelKind },
  parameters: certification({
    experience: 'lite',
    readyText: 'Preview',
    assertions: {
      visible: [
        'Preview',
        'find the legacy auth middleware',
        'Read',
        'Write',
        'Grep',
        'legacy auth middleware migration',
        'Shell',
        'preview clipped',
      ],
      ordered: [
        'find the legacy auth middleware',
        'Read',
        'Write',
        'Grep',
        'Shell',
        'preview clipped',
      ],
    },
    coversVisualStates: ['verbosity-lite'],
  }),
};

const truncationEditorAssertions: StorybookAssertions = {
  visible: ['Tool output', 'adjust', 'digits to set', 'u for unlimited'],
  ordered: ['Tool output', 'adjust', 'Preview'],
};

export const TruncationEditorTui = {
  args: { panel: 'truncation-tui' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: 'Tool output',
    assertions: {
      ...truncationEditorAssertions,
      visible: [
        ...(truncationEditorAssertions.visible ?? []),
        'Shell',
        'cat fixture.txt',
      ],
      hidden: ['line 60: lorem ipsum dolor sit amet'],
    },
    coversVisualStates: ['truncation-tui'],
  }),
};

export const TruncationEditorLite = {
  args: { panel: 'truncation-lite' satisfies PanelKind },
  parameters: certification({
    experience: 'lite',
    readyText: 'Tool output',
    assertions: {
      ...truncationEditorAssertions,
      visible: [
        ...(truncationEditorAssertions.visible ?? []),
        'line 60: lorem ipsum dolor sit amet',
      ],
      ordered: ['Tool output', 'adjust', 'Preview', 'line 60'],
    },
    coversVisualStates: ['truncation-lite'],
  }),
};

export const HelpCatalog = {
  args: { panel: 'help' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/help',
    assertions: { visible: ['/help'] },
    captures: {
      catalog: {
        label: 'complete command help catalog',
        coversVisualStates: ['help-catalog'],
        assertions: {
          visible: [
            'Usage: /',
            '<COMMAND>',
            'Type /guide',
            'chat',
            'workflow',
            'settings',
            'hooks',
            'config',
            'subcommands: save, load, resume',
          ],
          ordered: ['chat', 'workflow', 'settings', 'hooks', 'config'],
        },
      },
      filtered: {
        label: 'help catalog filtered by workflow',
        coversVisualStates: ['help-filtered'],
        assertions: {
          visible: ['workflow', 'Manage workflow runs and history'],
          hidden: [
            'Save, load, and inspect conversations',
            'Configure display and terminal preferences',
          ],
          occurrences: { 'Manage workflow runs and history': 1 },
        },
      },
    },
  }),
  play: captureHelpSearch,
};

export const HooksConfigured = {
  args: { panel: 'hooks' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/hooks',
    assertions: {
      visible: [
        '3 hooks',
        'Name',
        'Source',
        'Trigger',
        'release-guard',
        'cloud',
        'preToolUse',
        'test',
        'format-after-write',
        'postToolUse',
        'audit-prompt',
        'userPromptSubmit',
      ],
      ordered: ['format-after-write', 'release-guard', 'audit-prompt'],
    },
    coversVisualStates: ['hooks-configured'],
  }),
};

export const HooksEmpty = {
  args: { panel: 'hooks-empty' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: 'No hooks configured',
    assertions: {
      visible: ['/hooks', '0 hooks', 'No hooks configured'],
      hidden: ['Source', 'Trigger', 'Command', 'Matcher'],
    },
    coversVisualStates: ['hooks-empty'],
  }),
};

export const ConfigSummary = {
  args: { panel: 'config' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/config',
    assertions: {
      visible: [
        'Category',
        'Source',
        'Status',
        'agents',
        '2 available',
        'MCP servers',
        '1 active',
        'powers',
        '2 installed',
        'steering',
        '2 files',
        'skills',
        '2 skills',
        'hooks',
        '3 configured',
        'Cloud settings refresh is pending',
        'https://app.kiro.dev/settings',
      ],
      ordered: [
        'agents',
        'MCP servers',
        'powers',
        'steering',
        'skills',
        'hooks',
      ],
    },
    coversVisualStates: ['config-summary'],
  }),
};

export const ConfigSteering = {
  args: { panel: 'config-steering' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/config — steering',
    assertions: {
      visible: [
        'Name',
        'Source',
        'Inclusion',
        'architecture.md',
        'local',
        'always',
        'coding-standards.md',
        'cloud',
        'fileMatch',
        '~/.kiro/steering/',
      ],
      ordered: ['architecture.md', 'coding-standards.md'],
    },
    coversVisualStates: ['config-steering'],
  }),
};

export const ConfigSkills = {
  args: { panel: 'config-skills' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/config — skills',
    assertions: {
      visible: [
        'Name',
        'Source',
        'Description',
        'release-review',
        'Review release evidence',
        'test-authoring',
        'Build deterministic test',
        'scenarios',
        '~/.kiro/skills/',
      ],
      ordered: ['release-review', 'test-authoring'],
    },
    coversVisualStates: ['config-skills'],
  }),
};

export const ConfigPowers = {
  args: { panel: 'config-powers' satisfies PanelKind },
  parameters: certification({
    experience: 'tui',
    readyText: '/config — powers',
    assertions: {
      visible: [
        'Name',
        'Source',
        'Description',
        'Release certification',
        'Runs release-candidate',
        'verification',
        'Dependency audit',
        'Checks dependency health',
        '~/.kiro/powers/',
      ],
      ordered: ['Release certification', 'Dependency audit'],
    },
    coversVisualStates: ['config-powers'],
  }),
};
