import React from 'react';
import { Message, MessageType, type MessageProps } from './Message.js';
import { Card } from '../../ui/card/Card.js';

const meta = {
  component: Message,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'DeveloperMessage',
      'AgentMessage',
      'AgentWithMarkdown',
      'AgentWithTable',
      'SuccessMessage',
      'InfoMessage',
      'WarningMessage',
      'ErrorMessageWithStatus',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const DeveloperMessage = {
  parameters: {
    docs: {
      storyDescription: 'Developer message with brandMuted bar color',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: 'npm install react',
    type: MessageType.DEVELOPER,
  },
};

export const AgentMessage = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with brand bar color',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: 'I can help you with that!',
    type: MessageType.AGENT,
  },
};

export const AgentWithMarkdown = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with markdown formatting',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: 'Here is a **bold** statement with `code` and *italic* text.',
    type: MessageType.AGENT,
  },
};

export const AgentWithTable = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with a markdown table',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: `Here are the top countries by population:

| Country | Population | Average GDP (USD) |
|---------|-----------|-------------------|
| Aurelia | 48,200,000 | $1.2T |
| Belvoria | 112,500,000 | $3.8T |
| Caldonia | 9,800,000 | $420B |
| Dravenia | 67,300,000 | $2.1T |
| Estara | 23,100,000 | $890B |`,
    type: MessageType.AGENT,
  },
};

export const SuccessMessage = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with success status dot',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: 'Operation completed successfully\nAll files have been processed',
    type: MessageType.AGENT,
    status: 'success',
  },
};

export const InfoMessage = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with info status dot',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content:
      'Here is some additional information\nYou may want to review the documentation',
    type: MessageType.AGENT,
    status: 'info',
  },
};

export const WarningMessage = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with warning status dot',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content: 'Please proceed with caution\nThis action may have side effects',
    type: MessageType.AGENT,
    status: 'warning',
  },
};

export const ErrorMessageWithStatus = {
  parameters: {
    docs: {
      storyDescription: 'Agent message with error status dot',
    },
  },
  render: (args: MessageProps) => (
    <Card active={true}>
      <Message {...args} />
    </Card>
  ),
  args: {
    content:
      'Failed to connect to server\nPlease check your network connection',
    type: MessageType.AGENT,
    status: 'error',
  },
};
