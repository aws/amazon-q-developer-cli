import type { ComponentType } from 'react';
import type { ApprovalSurface } from './approval-surface.js';
import type { StatusSurface } from './status-surface.js';

export type ActivitySurface = ComponentType;

export interface VariantLayoutProps {
  ApprovalPrompt: ApprovalSurface;
  StatusLine: StatusSurface;
  ActivityTray: ActivitySurface;
}
