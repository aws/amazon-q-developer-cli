import { resolve } from 'node:path';
import type { LayoutChoice } from './layout.js';
import { BUILTIN_WIDGET_CAPABILITIES, BUILTIN_WIDGET_IDS } from './widgets.js';
import type { CanvasGroupState } from './workbench-state.js';

export function buildComposerPrompt({
  text,
  activeLayout,
  layouts,
  canvasGroups,
  composerRoot,
  composerSkill,
}: {
  text: string;
  activeLayout: LayoutChoice;
  layouts: LayoutChoice[];
  canvasGroups: Readonly<Record<string, CanvasGroupState>>;
  composerRoot: string;
  composerSkill: string;
}): string {
  const explicitLayout = /^\/layout(?:\s|$)/i.test(text);
  const request = explicitLayout ? text.replace(/^\/layout\s*/i, '').trim() : text.trim();
  const capabilities = BUILTIN_WIDGET_IDS.map((id) => `- ${id}: ${BUILTIN_WIDGET_CAPABILITIES[id]}`).join('\n');
  const availableLayouts = layouts.map((layout) => `${layout.spec.title}: ${layout.path}`).join('\n');
  const context = [
    'Mode: runtime composer',
    `User request: ${request || 'Guide the user in choosing a useful layout.'}`,
    `Invocation: ${explicitLayout ? 'explicit /layout command' : 'ambient composer chat'}`,
    `Active layout: ${activeLayout.path}`,
    `Layouts directory: ${resolve(composerRoot, 'layouts')}`,
    `Composer entry: ${resolve(composerRoot, 'index.tsx')}`,
    `Registered widgets: ${BUILTIN_WIDGET_IDS.join(', ')}`,
    `Widget capabilities:\n${capabilities}`,
    `Available layouts:\n${availableLayouts}`,
    `Current parsed layout JSON:\n${JSON.stringify(activeLayout.spec, null, 2)}`,
    `Current canvas tabs (runtime state, not layout nodes):\n${JSON.stringify(canvasGroups, null, 2)}`,
    'For ambient chat, compose only when the request concerns this composer UI, its layout, widgets, panes, tabs, theme, or settings. Otherwise answer normally and do not edit layout files.',
    'Prefer a JSON-only layout change. Create a new JSON file when the user asks for a new layout.',
  ].join('\n');
  return `Follow this embedded Twinki app composer skill exactly:\n\n${composerSkill.replace('$ARGUMENTS', context)}`;
}
