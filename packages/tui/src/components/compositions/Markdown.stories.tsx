import React from 'react';
import { Box } from '../../renderer.js';
import { Message, MessageType } from '../chat/message/Message.js';
import { Text } from '../ui/text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { buildRenderTheme, renderMessageToText } from '../../lite/render.js';
import type {
  StorybookAssertions,
  StorybookExperience,
  StorybookParameters,
} from '../../storybook/contracts.js';

interface MarkdownStoryProps {
  experience: StorybookExperience;
  content: string;
  prompt?: string;
  columns?: number;
}

const richDocument = `# Release review

The **parser** keeps *emphasis*, ~~obsolete~~ notes, and \`inlineCode\`.

Read [the rendering guide](https://example.com/rendering) before release.

- Inspect the transcript
  - Preserve nested context
1. Run the visual lane
2. Review the evidence

> Preserve the terminal contract.

---

The document is ready.`;

const codeAndTable = `## Verification matrix

| Lane | Platform | Result |
|:-----|:--------:|-------:|
| TUI | Linux | 18 |
| Lite | macOS | 24 |

\`\`\`ts
const certified = lanes.every((lane) => lane.passed);
console.log({ certified });
\`\`\``;

const narrowDocument = `### Narrow terminal

This paragraph intentionally contains enough words to exercise terminal wrapping without changing the logical markdown structure or losing inline \`code spans\`.

- First wrapped list item keeps its marker
- Second wrapped list item keeps its content`;

const streamingDocument = `## Streaming update

- first item is complete
- second item is still arriving

\`\`\`ts
const pending = true;`;

const meta = {
  title: 'Compositions/Markdown',
  component: MarkdownStory,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'TuiRichDocument',
      'LiteRichDocument',
      'TuiCodeAndTable',
      'LiteCodeAndTable',
      'TuiNarrowWrapping',
      'LiteNarrowWrapping',
      'TuiIncompleteStream',
      'LiteIncompleteStream',
      'TuiTranscript',
      'LiteTranscript',
    ],
    visualStates: {
      'rich-document': {
        label: 'Headings, inline styles, links, lists, quote, and rule',
      },
      'code-and-table': {
        label: 'Fenced source code and aligned table',
      },
      'narrow-wrapping': {
        label: 'Markdown wrapping in a narrow terminal',
      },
      'incomplete-stream': {
        label: 'Incomplete fenced block while markdown is streaming',
      },
      'integrated-transcript': {
        label: 'User prompt followed by a markdown assistant response',
      },
    },
  },
};

export default meta;

function MarkdownStory({
  experience,
  content,
  prompt,
  columns = 100,
}: MarkdownStoryProps): React.ReactElement {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const glyphs = useGlyphs();

  if (experience === 'tui') {
    return (
      <Box flexDirection="column" width={columns}>
        {prompt ? (
          <Message content={prompt} type={MessageType.DEVELOPER} />
        ) : null}
        <Message content={content} type={MessageType.AGENT} />
      </Box>
    );
  }

  const theme = buildRenderTheme(
    getColor,
    getUserPromptColor,
    getUserPromptBgHex
  );
  const rendered = [
    prompt
      ? renderMessageToText(
          { id: 'markdown-user', role: 'user', content: prompt },
          'Kiro',
          { glyphs, termCols: columns, theme }
        )
      : '',
    renderMessageToText(
      { id: 'markdown-model', role: 'model', content },
      'Kiro',
      { glyphs, termCols: columns, theme }
    ),
  ]
    .filter(Boolean)
    .join('\n\n');

  return (
    <Box flexDirection="column" width={columns}>
      <Text>{rendered}</Text>
    </Box>
  );
}

function certification(
  experience: StorybookExperience,
  readyText: string,
  assertions: StorybookAssertions,
  coversVisualStates: readonly string[],
  columns = 100
): StorybookParameters {
  return {
    experience,
    coversVisualStates,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport: { columns, rows: 34 },
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
    },
  };
}

const richAssertions: StorybookAssertions = {
  visible: [
    'Release review',
    'parser',
    'emphasis',
    'inlineCode',
    'rendering guide',
    'Inspect the transcript',
    'Preserve nested context',
    'Run the visual lane',
    'Preserve the terminal contract',
    'The document is ready',
  ],
  hidden: ['**parser**', '*emphasis*', '[the rendering guide]'],
  ordered: [
    'Release review',
    'parser',
    'rendering guide',
    'Inspect the transcript',
    'Preserve nested context',
    'Run the visual lane',
    'Preserve the terminal contract',
    'The document is ready',
  ],
};

export const TuiRichDocument = {
  args: { experience: 'tui', content: richDocument },
  parameters: certification('tui', 'Release review', richAssertions, [
    'rich-document',
  ]),
};

export const LiteRichDocument = {
  args: { experience: 'lite', content: richDocument },
  parameters: certification('lite', 'Release review', richAssertions, [
    'rich-document',
  ]),
};

const codeAndTableAssertions: StorybookAssertions = {
  visible: [
    'Verification matrix',
    'Lane',
    'Platform',
    'Result',
    'TUI',
    'Linux',
    'Lite',
    'macOS',
    'const certified = lanes.every',
    'console.log({ certified });',
  ],
  ordered: [
    'Verification matrix',
    'Lane',
    'TUI',
    'Lite',
    'const certified = lanes.every',
    'console.log({ certified });',
  ],
};

export const TuiCodeAndTable = {
  args: { experience: 'tui', content: codeAndTable },
  parameters: certification(
    'tui',
    'Verification matrix',
    codeAndTableAssertions,
    ['code-and-table']
  ),
};

export const LiteCodeAndTable = {
  args: { experience: 'lite', content: codeAndTable },
  parameters: certification(
    'lite',
    'Verification matrix',
    codeAndTableAssertions,
    ['code-and-table']
  ),
};

const narrowAssertions: StorybookAssertions = {
  visible: [
    'Narrow terminal',
    'exercise terminal wrapping',
    'markdown structure',
    'code spans',
    'First wrapped list item',
    'Second wrapped list item',
  ],
  ordered: [
    'Narrow terminal',
    'exercise terminal wrapping',
    'First wrapped list item',
    'Second wrapped list item',
  ],
};

export const TuiNarrowWrapping = {
  args: { experience: 'tui', content: narrowDocument, columns: 58 },
  parameters: certification(
    'tui',
    'Narrow terminal',
    narrowAssertions,
    ['narrow-wrapping'],
    58
  ),
};

export const LiteNarrowWrapping = {
  args: { experience: 'lite', content: narrowDocument, columns: 58 },
  parameters: certification(
    'lite',
    'Narrow terminal',
    narrowAssertions,
    ['narrow-wrapping'],
    58
  ),
};

const streamingAssertions: StorybookAssertions = {
  visible: [
    'Streaming update',
    'first item is complete',
    'second item is still arriving',
    'const pending = true;',
  ],
  ordered: [
    'Streaming update',
    'first item is complete',
    'second item is still arriving',
    'const pending = true;',
  ],
};

export const TuiIncompleteStream = {
  args: { experience: 'tui', content: streamingDocument },
  parameters: certification('tui', 'Streaming update', streamingAssertions, [
    'incomplete-stream',
  ]),
};

export const LiteIncompleteStream = {
  args: { experience: 'lite', content: streamingDocument },
  parameters: certification('lite', 'Streaming update', streamingAssertions, [
    'incomplete-stream',
  ]),
};

const transcriptPrompt =
  'Compare markdown rendering in both terminal experiences.';
const transcriptResponse = `### Rendering result

The response keeps **semantic emphasis** and a visible \`status: passed\` marker.

1. Parse the message
2. Render the terminal frame
3. Publish the evidence`;

const transcriptAssertions: StorybookAssertions = {
  visible: [
    transcriptPrompt,
    'Rendering result',
    'semantic emphasis',
    'status: passed',
    'Parse the message',
    'Render the terminal frame',
    'Publish the evidence',
  ],
  hidden: ['**semantic emphasis**'],
  ordered: [
    transcriptPrompt,
    'Rendering result',
    'semantic emphasis',
    'Parse the message',
    'Render the terminal frame',
    'Publish the evidence',
  ],
  occurrences: {
    [transcriptPrompt]: 1,
    'Rendering result': 1,
    'Publish the evidence': 1,
  },
};

export const TuiTranscript = {
  args: {
    experience: 'tui',
    prompt: transcriptPrompt,
    content: transcriptResponse,
  },
  parameters: certification('tui', transcriptPrompt, transcriptAssertions, [
    'integrated-transcript',
  ]),
};

export const LiteTranscript = {
  args: {
    experience: 'lite',
    prompt: transcriptPrompt,
    content: transcriptResponse,
  },
  parameters: certification('lite', transcriptPrompt, transcriptAssertions, [
    'integrated-transcript',
  ]),
};
