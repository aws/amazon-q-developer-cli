import type React from 'react';

export type StorybookKey =
  | 'enter'
  | 'escape'
  | 'tab'
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'ctrl+x'
  | 'ctrl+g';

export interface StorybookPlayContext {
  press(key: StorybookKey): Promise<void>;
  type(text: string, options?: { delayMs?: number }): Promise<void>;
  waitFor(text: string, timeoutMs?: number): Promise<void>;
  sleep(ms: number): Promise<void>;
  capture(label: string): Promise<void>;
}

export type StorybookPlay = (
  context: StorybookPlayContext
) => void | Promise<void>;

export interface StorybookViewport {
  columns: number;
  rows: number;
}

export interface StorybookAssertions {
  visible?: readonly string[];
  hidden?: readonly string[];
}

export interface StorybookCertification {
  suite: string;
  readyText: string;
  viewport?: StorybookViewport;
  assertions?: StorybookAssertions;
  settleMs?: number;
}

export interface StorybookParameters {
  layout?: 'fullscreen';
  capturesKeyboard?: boolean;
  optionalProps?: readonly string[];
  storyOrder?: readonly string[];
  certification?: StorybookCertification;
  docs?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface StorybookVariant {
  id: string;
  name: string;
  props: Record<string, unknown>;
  component?: React.ElementType;
  parameters: StorybookParameters;
  play?: StorybookPlay;
}

export interface StorybookDefinition {
  id: string;
  name: string;
  description: string;
  category: string;
  sourcePath: string;
  variants: StorybookVariant[];
  component: React.ElementType | null;
}

export interface StorybookSelection {
  storyId: string;
  variantId: string;
}

export interface ResolvedStorybookSelection {
  story: StorybookDefinition;
  variant: StorybookVariant;
}
