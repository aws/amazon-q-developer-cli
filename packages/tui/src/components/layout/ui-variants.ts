import type { ComponentType } from 'react';
import { InlineLayout } from './InlineLayout.js';
import { LiteLayout } from './lite/LiteLayout.js';
import { ActivityTray as TuiActivityTray } from '../ui/activity-tray/index.js';
import { LiteActivityTray } from './lite/LiteActivityTray.js';
import { LiteApprovalSurface, TuiApprovalSurface } from './approval-surface.js';
import { TuiStatusSurface } from './tui-status-surface.js';
import { LiteStatusSurface } from './lite/status-surface.js';
import type { VariantLayoutProps } from './variant-layout.js';
import type { UiMode } from '../../types/ui-mode.js';

/**
 * The single source of truth for genuinely forked UI surfaces. A surface
 * belongs here only when tui and lite have two distinct implementations, share
 * one selectable contract, and are selected at AppContainer's mode fork.
 *
 * Shared components that read uiMode to vary data stay shared. Incompatible
 * forks, such as React-node versus ANSI tool renderers, belong in the
 * tool-capabilities render registry rather than this table.
 */
export type VariantSurfaces = {
  Layout: ComponentType<VariantLayoutProps>;
} & VariantLayoutProps;

export const UI_VARIANTS = {
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
    ActivityTray: LiteActivityTray,
  },
} satisfies Record<UiMode, VariantSurfaces>;
