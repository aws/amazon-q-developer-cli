import React from 'react';
import { PromptInput } from './PromptInput.js';

const meta = {
  component: PromptInput,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Ready', 'Processing', 'WithSlashTrigger', 'WithMultipleTriggers', 'NoTriggers'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Ready = {
  parameters: {
    docs: {
      storyDescription: 'Default ready state for user input',
    },
    capturesKeyboard: true,
  },
  args: {
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
  },
};

export const Processing = {
  parameters: {
    docs: {
      storyDescription: 'Processing state with disabled input',
    },
    capturesKeyboard: true,
  },
  args: {
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: true,
  },
};

export const WithSlashTrigger = {
  parameters: {
    docs: {
      storyDescription: 'Input with slash trigger for commands',
    },
    capturesKeyboard: true,
  },
  args: {
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    triggerRules: [{ key: '/', type: 'start' }],
    onTriggerDetected: (trigger: any) => {
      if (trigger) {
        console.log('Slash trigger detected:', trigger);
      }
    },
  },
};

export const WithMultipleTriggers = {
  parameters: {
    docs: {
      storyDescription: 'Input with multiple trigger types (/, #, @)',
    },
    capturesKeyboard: true,
  },
  args: {
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    triggerRules: [
      { key: '/', type: 'start' },
      { key: '#', type: 'inline' },
      { key: '@', type: 'inline' },
    ],
    onTriggerDetected: (trigger: any) => {
      if (trigger) {
        console.log('Trigger detected:', trigger);
      }
    },
  },
};

export const NoTriggers = {
  parameters: {
    docs: {
      storyDescription: 'Basic input without any trigger functionality',
    },
    capturesKeyboard: true,
  },
  args: {
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    // No triggerRules - component works as basic input
  },
};
