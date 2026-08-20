#!/usr/bin/env node
import React from 'react';
import { render } from './../renderer.js';
import { AnimationPausedContext } from '../contexts/AnimationPausedContext.js';
import { Storybook } from './Storybook.js';
import { ThemeProvider } from '../theme/index.js';
import { Kiro } from '../kiro.js';
import { AppStoreContext, createAppStore } from '../stores/app-store.js';
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
const appStore = createAppStore({ kiro: new Kiro() });
const pauseAnimations = process.env.KIRO_STORYBOOK_PAUSE_ANIMATIONS === '1';

// Clear the terminal
process.stdout.write('\x1b[2J\x1b[0f');

// Run the storybook
const { unmount } = render(
  <AnimationPausedContext.Provider value={pauseAnimations}>
    <ThemeProvider>
      <AppStoreContext.Provider value={appStore}>
        <Storybook selection={selection} />
      </AppStoreContext.Provider>
    </ThemeProvider>
  </AnimationPausedContext.Provider>
);

// Handle exit
process.on('SIGINT', () => {
  unmount();
  process.exit(0);
});
