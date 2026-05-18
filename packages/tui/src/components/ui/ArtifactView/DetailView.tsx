import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { MarkdownRenderer } from '../MarkdownRenderer.js';
import type { OpenArtifactView } from '../../../stores/app-store.js';

interface Props {
  view: OpenArtifactView;
}

/**
 * Render the verbatim detailBody for the currently selected item, with
 * markdown formatting applied (headings, lists, code blocks, tables, etc).
 *
 * The parser produces `detailBody` as a verbatim slice of the source
 * markdown — heading + body for requirements/design, or the numbered
 * task plus its sub-list for tasks. `MarkdownRenderer` handles wrapping,
 * inline styles, syntax highlighting in fenced blocks, and table layout.
 */
export const DetailView: React.FC<Props> = ({ view }) => {
  const { getColor, getUserResponseColor } = useTheme();
  const dim = getColor('secondary');
  const body = pickDetailBody(view);
  if (body == null) {
    return (
      <Box marginTop={1}>
        <Text>{dim('No detail available for this item.')}</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <MarkdownRenderer content={body} color={getUserResponseColor()} />
    </Box>
  );
};

function pickDetailBody(view: OpenArtifactView): string | null {
  const summary = view.summary;
  if (summary.kind === 'requirements') {
    const item = summary.items[view.cursor];
    return item?.detailBody ?? null;
  }
  if (summary.kind === 'design') {
    const section = summary.sections[view.cursor];
    return section?.detailBody ?? null;
  }
  // tasks
  const task = summary.items[view.cursor];
  return task?.detailBody ?? null;
}
