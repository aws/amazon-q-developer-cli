#!/usr/bin/env node
import React from 'react';
import { render } from 'ink';
import { Storybook } from './Storybook.js';
import { ThemeProvider } from '../theme/index.js';


// Clear the terminal
process.stdout.write('\x1b[2J\x1b[0f');

// Run the storybook
const { unmount } = render(
  <ThemeProvider>
    <Storybook />
  </ThemeProvider>,
);

// Handle exit
process.on('SIGINT', () => {
  unmount();
  process.exit(0);
});
