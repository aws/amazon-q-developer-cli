import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { OpenArtifactView } from '../../../stores/app-store.js';

interface Props {
  view: OpenArtifactView;
}

/** Render the verbatim detailBody for the currently selected item. */
export const DetailView: React.FC<Props> = ({ view }) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const body = pickDetailBody(view);
  if (body == null) {
    return (
      <Box marginTop={1}>
        <Text>{dim('No detail available for this item.')}</Text>
      </Box>
    );
  }

  // Render the detail body line-by-line so long markdown bodies wrap
  // gracefully under twinki's flexbox layout.
  const lines = body.split('\n');
  return (
    <Box flexDirection="column">
      {lines.map((line, i) => (
        <Text key={i}>{line.length === 0 ? ' ' : line}</Text>
      ))}
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
