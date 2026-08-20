import type React from 'react';

export type StorybookKey =
  | 'enter'
  | 'escape'
  | 'backspace'
  | 'tab'
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'shift+up'
  | 'shift+down'
  | 'shift+left'
  | 'shift+right'
  | 'shift+tab'
  | 'ctrl+p'
  | 'ctrl+x'
  | 'ctrl+g';

export interface StorybookPlayContext {
  press(key: StorybookKey): Promise<void>;
  type(text: string, options?: { delayMs?: number }): Promise<void>;
  waitFor(text: string, timeoutMs?: number): Promise<void>;
  sleep(ms: number): Promise<void>;
  capture(id: string): Promise<void>;
}

export type StorybookPlay = (
  context: StorybookPlayContext
) => void | Promise<void>;

export interface StorybookViewport {
  columns: number;
  rows: number;
}

export type StorybookExperience = 'tui' | 'lite';

export type StorybookTerminalColor =
  | 'default'
  | `#${string}`
  | `palette:${number}`;

export interface StorybookTextStyleAssertion {
  text: string;
  occurrence?: number;
  foreground?: StorybookTerminalColor;
  background?: StorybookTerminalColor;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
  inverse?: boolean;
}

export interface StorybookAssertions {
  visible?: readonly string[];
  hidden?: readonly string[];
  ordered?: readonly string[];
  occurrences?: Readonly<Record<string, number>>;
  styled?: readonly StorybookTextStyleAssertion[];
}

export interface StorybookCaptureDefinition {
  label: string;
  assertions?: StorybookAssertions;
  coversVisualStates?: readonly string[];
}

export interface StorybookVisualStateDefinition {
  label: string;
  description?: string;
  gapType?:
    | 'missing-story'
    | 'integration-only'
    | 'product-limitation'
    | 'visual-baseline-required';
}

export interface StorybookCertification {
  suite: string;
  readyText: string;
  viewport?: StorybookViewport;
  environment?: Readonly<Record<string, string>>;
  assertions?: StorybookAssertions;
  captures?: Readonly<Record<string, StorybookCaptureDefinition>>;
  settleMs?: number;
}

export interface StorybookParameters {
  layout?: 'fullscreen';
  experience?: StorybookExperience;
  capturesKeyboard?: boolean;
  optionalProps?: readonly string[];
  storyOrder?: readonly string[];
  visualStates?: Readonly<Record<string, StorybookVisualStateDefinition>>;
  coversVisualStates?: readonly string[];
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
