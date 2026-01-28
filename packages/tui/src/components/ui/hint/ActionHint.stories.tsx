import React from 'react';
import { ActionHint, type ActionHintProps } from './ActionHint.js';

const meta = {
  component: ActionHint,
  parameters: {
    layout: 'fullscreen',
  },
  tags: ['autodocs'],
};

export default meta;

export const ExitHint = {
  args: {
    text: 'Press Ctrl+C again to exit',
  } as ActionHintProps,
};

export const CancelHint = {
  args: {
    text: 'Press Esc to cancel',
  } as ActionHintProps,
};

export const ConfirmHint = {
  args: {
    text: 'Press Enter to confirm',
  } as ActionHintProps,
};

export const NavigationHint = {
  args: {
    text: 'Use arrow keys to navigate, Enter to select',
  } as ActionHintProps,
};
