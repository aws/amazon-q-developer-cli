import { Introspect } from './Introspect.js';

const meta = {
  component: Introspect,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Introspecting',
      'IntrospectedQuery',
      'IntrospectedDocPath',
      'NoQuery',
      'Error',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// In-progress state
export const Introspecting = {
  args: {
    content: JSON.stringify({ query: 'MCP governance' }),
    isFinished: false,
  },
};

// Finished with query
export const IntrospectedQuery = {
  args: {
    content: JSON.stringify({ query: 'how to use slash commands' }),
    isFinished: true,
  },
};

// Finished with doc_path
export const IntrospectedDocPath = {
  args: {
    content: JSON.stringify({ doc_path: 'features/tangent-mode.md' }),
    isFinished: true,
  },
};

// No query provided
export const NoQuery = {
  args: {
    content: JSON.stringify({}),
    isFinished: true,
  },
};

// Error state
export const Error = {
  args: {
    content: JSON.stringify({ query: 'some failing query' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Semantic search failed: index not found',
    },
  },
};
