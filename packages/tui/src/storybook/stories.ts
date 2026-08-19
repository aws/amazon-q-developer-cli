import type React from 'react';
import type {
  ResolvedStorybookSelection,
  StorybookDefinition,
  StorybookParameters,
  StorybookPlay,
  StorybookSelection,
} from './contracts.js';

// Import all component stories in Storybook format
import * as CardStories from '../components/ui/card/Card.stories.js';
import * as WelcomeScreenStories from '../components/welcome-screen/WelcomeScreen.stories.js';
import * as TextStories from '../components/ui/text/Text.stories.js';
import * as WordmarkStories from '../components/brand/Wordmark.stories.js';
import * as PromptInputStories from '../components/chat/prompt-bar/PromptInput.stories.js';
import * as ContextBarStories from '../components/chat/prompt-bar/ContextBar.stories.js';
import * as StatusBarStories from '../components/chat/status-bar/StatusBar.stories.js';
import * as PromptBarStories from '../components/chat/prompt-bar/PromptBar.stories.js';
import * as MenuStories from '../components/ui/menu/Menu.stories.js';
import * as ChipStories from '../components/ui/chip/Chip.stories.js';
import * as ProgressChipStories from '../components/ui/chip/ProgressChip.stories.js';
import * as MessageStories from '../components/chat/message/Message.stories.js';
import * as ThinkingDisplayStories from '../components/chat/message/ThinkingDisplay.stories.js';
import * as RadioButtonStories from '../components/ui/radio/RadioButton.stories.js';
import * as RadioGroupStories from '../components/ui/radio/RadioGroup.stories.js';
import * as SnackBarStories from '../components/chat/prompt-bar/SnackBar.stories.js';
import * as PastedChipStories from '../components/chat/prompt-bar/PastedChip.stories.js';
import * as IconStories from '../components/ui/icon/Icon.stories.js';
import * as DividerStories from '../components/ui/divider/Divider.stories.js';
import * as WriteStories from '../components/chat/tools/Write.stories.js';
import * as GrepStories from '../components/chat/tools/Grep.stories.js';
import * as GlobStories from '../components/chat/tools/Glob.stories.js';
import * as StatusInfoStories from '../components/ui/status/StatusInfo.stories.js';
import * as AlertStories from '../components/ui/alert/Alert.stories.js';
import * as ShellStories from '../components/chat/tools/Shell.stories.js';
import * as ReadStories from '../components/chat/tools/Read.stories.js';
import * as WebSearchStories from '../components/chat/tools/WebSearch.stories.js';
import * as WebFetchStories from '../components/chat/tools/WebFetch.stories.js';
import * as IntrospectStories from '../components/chat/tools/Introspect.stories.js';
import * as FileListStories from '../components/chat/tools/FileList.stories.js';
import * as ToolMetaStories from '../components/chat/tools/ToolMeta.stories.js';
import * as NotificationBarStories from '../components/chat/notification-bar/NotificationBar.stories.js';
import * as ActionHintStories from '../components/ui/hint/ActionHint.stories.js';
import * as TableStories from '../components/ui/table/Table.stories.js';
import * as PieSpinnerStories from '../components/ui/spinner/PieSpinner.stories.js';
import * as SpinnerStories from '../components/ui/spinner/Spinner.stories.js';
import * as ToolStories from '../components/chat/tools/Tool.stories.js';
import * as ImageReadStories from '../components/chat/tools/ImageRead.stories.js';
import * as LsStories from '../components/chat/tools/Ls.stories.js';
import * as WorkflowMonitorStories from '../components/layout/workflow-monitor/WorkflowMonitorScreen.stories.js';
import * as WorkflowHistoryPanelStories from '../components/layout/workflow-monitor/WorkflowHistoryPanel.stories.js';
import * as ActivityTrayStories from '../components/ui/activity-tray/ActivityTray.stories.js';
import * as WorkflowToolStories from '../components/chat/tools/WorkflowTool.stories.js';
import * as WorkflowLifecycleRowStories from '../components/ui/WorkflowLifecycleRow.stories.js';
import * as CrewMonitorStories from '../components/multi-agent/CrewMonitor.stories.js';

interface RawStory {
  args?: Record<string, unknown>;
  component?: React.ElementType;
  parameters?: StorybookParameters;
  play?: StorybookPlay;
  render?: React.ElementType;
}

interface RawStoryMeta {
  component?: React.ElementType | null;
  parameters?: StorybookParameters;
  title?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function slug(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}

function rawStory(value: unknown): RawStory | null {
  return isRecord(value) ? (value as RawStory) : null;
}

function rawMeta(value: unknown): RawStoryMeta | null {
  return isRecord(value) ? (value as RawStoryMeta) : null;
}

/**
 * Derives a title from the import path if meta.title is not provided.
 * Format: "ParentFolder/SubFolder/ComponentName" or "ParentFolder/ComponentName"
 * Example: "../components/bar/StatusBar.stories.js" -> "Components/StatusBar"
 * Example: "../ui/chip/Chip.stories.js" -> "UI/Chip"
 * Example: "../ui/radio/RadioButton.stories.js" -> "UI/Radio/RadioButton"
 */
function deriveTitleFromPath(importPath: string): string {
  // Try to match components folder pattern
  let match = importPath.match(/components\/([^/]+)\/([^/]+)\.stories/);
  if (match) {
    const [, , component] = match;
    // Use "Components" as the category for all components in original-ui/components
    return `Components/${component!}`;
  }

  // Try to match ui folder pattern
  match = importPath.match(/\/ui\/([^/]+)\/([^/]+)\.stories/);
  if (match) {
    const [, folder, component] = match;
    // Capitalize folder name for display
    const folderName = folder!.charAt(0).toUpperCase() + folder!.slice(1);
    // Use "UI/FolderName/ComponentName" format
    return `UI/${folderName}/${component!}`;
  }

  // Try to match layout folder pattern
  match = importPath.match(/\/layout\/([^/]+)\/([^/]+)\.stories/);
  if (match) {
    const [, folder, component] = match;
    const folderName = folder!.charAt(0).toUpperCase() + folder!.slice(1);
    return `Layout/${folderName}/${component!}`;
  }

  // Try to match chat folder pattern
  match = importPath.match(/\/chat\/([^/]+)\/([^/]+)\.stories/);
  if (match) {
    const [, folder, component] = match;
    const folderName = folder!.charAt(0).toUpperCase() + folder!.slice(1);
    return `Chat/${folderName}/${component!}`;
  }

  // Try to match brand folder pattern
  match = importPath.match(/\/brand\/([^/]+)\.stories/);
  if (match) {
    const [, component] = match;
    return `Brand/${component!}`;
  }

  // Try to match welcome-screen folder pattern
  match = importPath.match(/\/welcome-screen\/([^/]+)\.stories/);
  if (match) {
    const [, component] = match;
    return `WelcomeScreen/${component!}`;
  }

  return 'Uncategorized/Component';
}

// Convert Storybook format to our internal format
function convertStoryModule(
  storyModule: Record<string, unknown>,
  importPath: string
): StorybookDefinition {
  const { default: metaValue, ...storyExports } = storyModule;
  const meta = rawMeta(metaValue);

  if (!meta) {
    throw new Error('Story module missing meta object');
  }

  // Use provided title or derive from import path
  const title = meta.title || deriveTitleFromPath(importPath);
  const componentName = title.split('/').pop() ?? 'Component';

  // Check if the meta has a custom story order defined
  const customOrder = meta.parameters?.storyOrder;

  let storyEntries = Object.entries(storyExports).flatMap(([name, value]) => {
    const story = rawStory(value);
    return story ? ([[name, story]] as const) : [];
  });

  // Apply custom ordering if defined in meta.parameters.storyOrder
  if (customOrder && Array.isArray(customOrder)) {
    storyEntries = customOrder.flatMap((name) => {
      const story = rawStory(storyExports[name]);
      return story ? ([[name, story]] as const) : [];
    });
  }

  // Get component name safely - handle React.memo wrapped components
  const getComponentName = (component: unknown): string => {
    if (!component) return componentName;
    if (typeof component === 'function') {
      const namedComponent = component as {
        displayName?: string;
        name?: string;
      };
      return namedComponent.displayName || namedComponent.name || componentName;
    }
    if (!isRecord(component)) return componentName;
    if (typeof component.displayName === 'string') return component.displayName;
    const wrapped = component.type;
    return typeof wrapped === 'function' && wrapped.name
      ? wrapped.name
      : componentName;
  };

  return {
    id: slug(title),
    name: componentName, // Get component name from title
    description: getComponentName(meta.component) + ' component',
    category: title, // Use full title as category (e.g., "UI/Radio/RadioButton")
    sourcePath: importPath,
    variants: storyEntries.map(([name, story]) => ({
      id: slug(name),
      name,
      props: story.args || {},
      component: story.render || story.component,
      parameters: { ...(meta.parameters || {}), ...(story.parameters || {}) },
      play: story.play,
    })),
    component: meta.component ?? null,
  };
}

export const stories = [
  convertStoryModule(CardStories, '../components/ui/card/Card.stories.js'),
  convertStoryModule(
    WelcomeScreenStories,
    '../components/welcome-screen/WelcomeScreen.stories.js'
  ),
  convertStoryModule(TextStories, '../components/ui/text/Text.stories.js'),
  convertStoryModule(
    WordmarkStories,
    '../components/brand/Wordmark.stories.js'
  ),
  convertStoryModule(
    PromptInputStories,
    '../components/chat/prompt-bar/PromptInput.stories.js'
  ),
  convertStoryModule(
    ContextBarStories,
    '../components/chat/prompt-bar/ContextBar.stories.js'
  ),
  convertStoryModule(
    StatusBarStories,
    '../components/chat/status-bar/StatusBar.stories.js'
  ),
  convertStoryModule(
    PromptBarStories,
    '../components/chat/prompt-bar/PromptBar.stories.js'
  ),
  convertStoryModule(MenuStories, '../components/ui/menu/Menu.stories.js'),
  convertStoryModule(ChipStories, '../components/ui/chip/Chip.stories.js'),
  convertStoryModule(
    ProgressChipStories,
    '../components/ui/chip/ProgressChip.stories.js'
  ),
  convertStoryModule(
    MessageStories,
    '../components/chat/message/Message.stories.js'
  ),
  convertStoryModule(
    ThinkingDisplayStories,
    '../components/chat/message/ThinkingDisplay.stories.js'
  ),
  convertStoryModule(
    RadioButtonStories,
    '../components/ui/radio/RadioButton.stories.js'
  ),
  convertStoryModule(
    RadioGroupStories,
    '../components/ui/radio/RadioGroup.stories.js'
  ),
  convertStoryModule(
    SnackBarStories,
    '../components/chat/prompt-bar/SnackBar.stories.js'
  ),
  convertStoryModule(
    PastedChipStories,
    '../components/chat/prompt-bar/PastedChip.stories.js'
  ),
  convertStoryModule(IconStories, '../components/ui/icon/Icon.stories.js'),
  convertStoryModule(
    DividerStories,
    '../components/ui/divider/Divider.stories.js'
  ),
  convertStoryModule(WriteStories, '../components/chat/tools/Write.stories.js'),
  convertStoryModule(GrepStories, '../components/chat/tools/Grep.stories.js'),
  convertStoryModule(GlobStories, '../components/chat/tools/Glob.stories.js'),
  convertStoryModule(
    StatusInfoStories,
    '../components/ui/status/StatusInfo.stories.js'
  ),
  convertStoryModule(AlertStories, '../components/ui/alert/Alert.stories.js'),
  convertStoryModule(ShellStories, '../components/chat/tools/Shell.stories.js'),
  convertStoryModule(ReadStories, '../components/chat/tools/Read.stories.js'),
  convertStoryModule(
    WebSearchStories,
    '../components/chat/tools/WebSearch.stories.js'
  ),
  convertStoryModule(
    WebFetchStories,
    '../components/chat/tools/WebFetch.stories.js'
  ),
  convertStoryModule(
    IntrospectStories,
    '../components/chat/tools/Introspect.stories.js'
  ),
  convertStoryModule(
    FileListStories,
    '../components/chat/tools/FileList.stories.js'
  ),
  convertStoryModule(
    ToolMetaStories,
    '../components/chat/tools/ToolMeta.stories.js'
  ),
  convertStoryModule(
    NotificationBarStories,
    '../components/chat/notification-bar/NotificationBar.stories.js'
  ),
  convertStoryModule(
    ActionHintStories,
    '../components/ui/hint/ActionHint.stories.js'
  ),
  convertStoryModule(TableStories, '../components/ui/table/Table.stories.js'),
  convertStoryModule(
    PieSpinnerStories,
    '../components/ui/spinner/PieSpinner.stories.js'
  ),
  convertStoryModule(
    SpinnerStories,
    '../components/ui/spinner/Spinner.stories.js'
  ),
  convertStoryModule(ToolStories, '../components/chat/tools/Tool.stories.js'),
  convertStoryModule(
    ImageReadStories,
    '../components/chat/tools/ImageRead.stories.js'
  ),
  convertStoryModule(LsStories, '../components/chat/tools/Ls.stories.js'),
  convertStoryModule(
    WorkflowMonitorStories,
    '../components/layout/workflow-monitor/WorkflowMonitorScreen.stories.js'
  ),
  convertStoryModule(
    WorkflowHistoryPanelStories,
    '../components/layout/workflow-monitor/WorkflowHistoryPanel.stories.js'
  ),
  convertStoryModule(
    ActivityTrayStories,
    '../components/ui/activity-tray/ActivityTray.stories.js'
  ),
  convertStoryModule(
    WorkflowToolStories,
    '../components/chat/tools/WorkflowTool.stories.js'
  ),
  convertStoryModule(
    WorkflowLifecycleRowStories,
    '../components/ui/WorkflowLifecycleRow.stories.js'
  ),
  convertStoryModule(
    CrewMonitorStories,
    '../components/multi-agent/CrewMonitor.stories.js'
  ),
];

export function resolveStorybookSelection(
  selection: StorybookSelection
): ResolvedStorybookSelection | null {
  const story = stories.find((candidate) => candidate.id === selection.storyId);
  const variant = story?.variants.find(
    (candidate) => candidate.id === selection.variantId
  );
  return story && variant ? { story, variant } : null;
}
