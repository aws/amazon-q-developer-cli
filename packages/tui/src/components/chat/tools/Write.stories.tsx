import React from 'react';
import { Write } from './Write.js';
import { Card } from '../../ui/card/Card.js';

const meta = {
  component: Write,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['InCard', 'InCardLarge', 'Standalone', 'StandaloneLarge'],
  },
  tags: ['autodocs'],
};

export default meta;

// Sample code for diff examples
const simpleOldCode = `function greet(name) {
  console.log("Hello " + name);
}`;

const simpleNewCode = `function greet(name) {
  console.log("Hello, " + name + "!");
}`;

const complexOldCode = `function calculateSum(a, b) {
  return a + b;
}

function multiply(x, y) {
  return x * y;
}

const result = calculateSum(5, 3);
console.log(result);
export { calculateSum };`;

const complexNewCode = `function calculateSum(a, b) {
  return a + b;
}

function multiply(x, y) {
  return x * y;
}

function divide(x, y) {
  return x / y;
}

const result = calculateSum(5, 3);
console.log('Result:', result);
export { calculateSum, divide };`;

// Write inside Card context (shows colored bars)
export const InCard = {
  parameters: {
    docs: {
      storyDescription:
        'Write inside Card - shows red/green/purple bars for removed/added/unchanged lines',
    },
  },
  render: () => (
    <Card active={true}>
      <Write oldText={simpleOldCode} newText={simpleNewCode} filePath="greet.js" />
    </Card>
  ),
};

export const InCardLarge = {
  parameters: {
    docs: {
      storyDescription: 'Larger Write inside Card - shows complex diff with colored bars',
    },
  },
  render: () => (
    <Card active={true}>
      <Write oldText={complexOldCode} newText={complexNewCode} filePath="utils.js" />
    </Card>
  ),
};

// Write standalone (without Card context)
export const Standalone = {
  parameters: {
    docs: {
      storyDescription: 'Write standalone (no Card context, no bars)',
    },
  },
  args: {
    oldText: simpleOldCode,
    newText: simpleNewCode,
    filePath: 'greet.js',
  },
};

export const StandaloneLarge = {
  parameters: {
    docs: {
      storyDescription: 'Larger Write standalone (no Card context, no bars)',
    },
  },
  args: {
    oldText: complexOldCode,
    newText: complexNewCode,
    filePath: 'utils.js',
  },
};
