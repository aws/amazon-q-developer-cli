import React from 'react';
import { WelcomeScreen } from './WelcomeScreen.js';

const meta = {
  component: WelcomeScreen,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Default', 'WithAnimation', 'NoServers', 'ManyServers'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Default = {
  parameters: {
    docs: {
      storyDescription: 'Default welcome screen with basic configuration',
    },
  },
  args: {
    agent: 'Kiro CLI',
    mcpServers: ['fs', 'git'],
    animate: false,
  },
};

export const WithAnimation = {
  parameters: {
    docs: {
      storyDescription: 'Welcome screen with animated wordmark',
    },
  },
  args: {
    agent: 'Kiro CLI',
    mcpServers: ['fs', 'git'],
    animate: true,
  },
};

export const NoServers = {
  parameters: {
    docs: {
      storyDescription: 'Welcome screen with no MCP servers configured',
    },
  },
  args: {
    agent: 'Kiro CLI',
    mcpServers: [],
    animate: false,
  },
};

export const ManyServers = {
  parameters: {
    docs: {
      storyDescription: 'Welcome screen with multiple MCP servers',
    },
  },
  args: {
    agent: 'PowerAgent',
    mcpServers: ['fs', 'git', 'docker', 'aws', 'database', 'api'],
    animate: false,
  },
};
