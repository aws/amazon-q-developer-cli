import React from 'react';
import { Card } from './Card.js';
import { Message, MessageType } from '../../chat/message/Message.js';
import { StatusBar } from '../../chat/status-bar/StatusBar.js';

const meta = {
  component: Card,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Basic', 'WithAnimation', 'WithStatusDots', 'Inactive'],
  },
  tags: ['autodocs'],
};

export default meta;

// 1. Basic conversation - no animation
export const Basic = {
  parameters: {
    docs: {
      storyDescription: 'Basic Card with developer question and agent response',
    },
  },
  args: {
    children: [
      React.createElement(Message, {
        content: 'Can you tell me a coding joke',
        type: MessageType.DEVELOPER,
        key: 'question',
      }),
      React.createElement(Message, {
        content:
          'Why do programmers prefer dark mode?\n\nBecause light attracts bugs! 🐛',
        type: MessageType.AGENT,
        key: 'answer',
      }),
    ],
    active: true,
  },
};

// 2. With animation
export const WithAnimation = {
  parameters: {
    docs: {
      storyDescription: 'Same conversation with animation enabled',
    },
  },
  args: {
    children: [
      React.createElement(Message, {
        content: 'Can you tell me a coding joke',
        type: MessageType.DEVELOPER,
        key: 'question',
      }),
      React.createElement(Message, {
        content:
          'Why do programmers prefer dark mode?\n\nBecause light attracts bugs! 🐛',
        type: MessageType.AGENT,
        key: 'answer',
      }),
    ],
    active: true,
    animated: true,
  },
};

// 3. With status dots
export const WithStatusDots = {
  parameters: {
    docs: {
      storyDescription: 'StatusBar with status dots showing different states',
    },
  },
  args: {
    children: [
      React.createElement(Message, {
        content: 'Can you tell me a coding joke',
        type: MessageType.DEVELOPER,
        status: 'info',
        key: 'question',
      }),
      React.createElement(Message, {
        content:
          'Why do programmers prefer dark mode?\n\nBecause light attracts bugs! 🐛',
        type: MessageType.AGENT,
        status: 'success',
        key: 'answer',
      }),
    ],
    active: true,
  },
};

// 4. Inactive state
export const Inactive = {
  parameters: {
    docs: {
      storyDescription:
        'Inactive Card - StatusBar shows empty space instead of colored bars',
    },
  },
  args: {
    children: [
      React.createElement(Message, {
        content: 'Can you tell me a coding joke',
        type: MessageType.DEVELOPER,
        key: 'question',
      }),
      React.createElement(Message, {
        content:
          'Why do programmers prefer dark mode?\n\nBecause light attracts bugs! 🐛',
        type: MessageType.AGENT,
        key: 'answer',
      }),
    ],
    active: false,
  },
};
