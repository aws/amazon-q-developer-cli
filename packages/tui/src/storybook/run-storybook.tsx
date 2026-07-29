#!/usr/bin/env node
import React from 'react';
import { render } from './../renderer.js';
import { Storybook } from './Storybook.js';
import { ThemeProvider } from '../theme/index.js';
import type { StorybookSelection } from './contracts.js';

function storybookSelectionFromEnvironment(): StorybookSelection | undefined {
  const storyId = process.env.KIRO_STORYBOOK_STORY;
  const variantId = process.env.KIRO_STORYBOOK_VARIANT;
  if (!storyId && !variantId) return undefined;
  if (!storyId || !variantId) {
    throw new Error(
      'KIRO_STORYBOOK_STORY and KIRO_STORYBOOK_VARIANT must be set together'
    );
  }
  return { storyId, variantId };
}

const selection = storybookSelectionFromEnvironment();

// Clear the terminal
process.stdout.write('\x1b[2J\x1b[0f');

// Run the storybook
const { unmount } = render(
  <ThemeProvider>
    <Storybook selection={selection} />
  </ThemeProvider>
);

// Handle exit
process.on('SIGINT', () => {
  unmount();
  process.exit(0);
});
