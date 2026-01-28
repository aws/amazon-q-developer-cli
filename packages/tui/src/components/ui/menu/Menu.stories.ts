import React from 'react';
import { Menu } from './Menu.js';

const meta = {
  component: Menu,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'SlashCommands',
      'ScrollingItems',
      'MinimalItems',
      'Mentions',
      'WithSelectedIndicator',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

const slashCommands = [
  { label: 'help', description: 'Show available commands and their usage' },
  { label: 'model', description: 'Select AI model for the chat session' },
  { label: 'quit', description: 'Exit the application' },
  { label: 'steering', description: 'Manage steering rules and configurations' },
  { label: 'spec', description: 'Create and manage feature specifications' },
  { label: 'hooks', description: 'Manage agent hooks and automations' },
  { label: 'config', description: 'Configure application settings' },
  { label: 'workspace', description: 'Manage workspace settings' },
];

const mentions = [
  { label: 'john', description: 'John Smith - Software Engineer' },
  { label: 'sarah', description: 'Sarah Johnson - Product Manager' },
  { label: 'team-frontend', description: 'Frontend Development Team' },
  { label: 'team-backend', description: 'Backend Development Team' },
];

const aiModels = [
  { label: 'GPT-4', description: 'OpenAI most capable model for complex tasks' },
  { label: 'GPT-4 Turbo', description: 'Faster and more cost-effective GPT-4' },
  { label: 'GPT-3.5 Turbo', description: 'Fast and efficient for most tasks' },
  { label: 'Claude 3.5 Sonnet', description: 'Anthropic balanced model for various tasks' },
  { label: 'Claude 3 Opus', description: 'Anthropic most powerful model' },
  { label: 'Claude 3 Haiku', description: 'Anthropic fastest and most compact model' },
  { label: 'Gemini Pro', description: 'Google multimodal AI model' },
  { label: 'Gemini Ultra', description: 'Google most capable model' },
  { label: 'Llama 3', description: 'Meta open-source language model' },
  { label: 'Llama 3.1', description: 'Meta improved open-source model' },
  { label: 'Mistral Large', description: 'Mistral AI flagship model' },
  { label: 'Mistral Medium', description: 'Mistral AI balanced performance model' },
  { label: 'Mixtral 8x7B', description: 'Mistral AI mixture of experts model' },
  { label: 'Command R+', description: 'Cohere enterprise-grade model' },
  { label: 'Command R', description: 'Cohere retrieval-augmented model' },
];

const manyCommands = [
  { label: 'help', description: 'Show available commands and their usage' },
  { label: 'model', description: 'Select AI model for the chat session' },
  { label: 'quit', description: 'Exit the application' },
  { label: 'steering', description: 'Manage steering rules and configurations' },
  { label: 'spec', description: 'Create and manage feature specifications' },
  { label: 'hooks', description: 'Manage agent hooks and automations' },
  { label: 'config', description: 'Configure application settings' },
  { label: 'workspace', description: 'Manage workspace settings' },
  { label: 'debug', description: 'Enable debug mode and logging' },
  { label: 'clear', description: 'Clear the current conversation' },
  { label: 'history', description: 'View conversation history' },
  { label: 'export', description: 'Export conversation or data' },
  { label: 'import', description: 'Import configuration or data' },
  { label: 'reset', description: 'Reset application to defaults' },
  { label: 'version', description: 'Show application version information' },
  { label: 'theme', description: 'Change the application theme' },
  { label: 'plugins', description: 'Manage installed plugins' },
  { label: 'search', description: 'Search through conversation history' },
  { label: 'backup', description: 'Create a backup of your data' },
  { label: 'restore', description: 'Restore data from backup' },
];

export const SlashCommands = {
  parameters: {
    docs: {
      storyDescription: 'Menu configured for slash commands with "/" prefix',
    },
  },
  args: {
    items: slashCommands,
    prefix: '/',
    onSelect: (item: any) => console.log('Selected command:', item),
  },
};

export const ScrollingItems = {
  parameters: {
    docs: {
      storyDescription:
        'Menu with many items to demonstrate scrolling behavior (20 items, 8 visible)',
    },
  },
  args: {
    items: manyCommands,
    prefix: '/',
    visibleItems: 8, // Show 8 items out of 20 total
    onSelect: (item: any) => console.log('Selected item:', item),
  },
};

export const MinimalItems = {
  parameters: {
    docs: {
      storyDescription: 'Menu with fewer items to show basic functionality',
    },
  },
  args: {
    items: slashCommands.slice(0, 3),
    prefix: '/',
    visibleItems: 5,
    onSelect: (item: any) => console.log('Selected item:', item),
  },
};

export const Mentions = {
  parameters: {
    docs: {
      storyDescription: 'Menu configured for mentions with "@" prefix',
    },
  },
  args: {
    items: mentions,
    prefix: '@',
    onSelect: (item: any) => console.log('Selected mention:', item),
  },
};

export const WithSelectedIndicator = {
  parameters: {
    docs: {
      storyDescription:
        'Menu with chevron indicator showing the selected item (15 AI models, 8 visible)',
    },
  },
  args: {
    items: aiModels,
    prefix: '',
    visibleItems: 8,
    showSelectedIndicator: true,
    onSelect: (item: any) => console.log('Selected model:', item),
  },
};
