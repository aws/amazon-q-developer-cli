import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { RequirementSummaryItem } from './RequirementSummaryItem.js';
import { DesignSummaryItem } from './DesignSummaryItem.js';
import { TaskSummaryItem } from './TaskSummaryItem.js';
import { EmptyState } from './EmptyState.js';
import type { OpenArtifactView } from '../../../stores/app-store.js';

interface Props {
  view: OpenArtifactView;
}

export const SummaryView: React.FC<Props> = ({ view }) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const summary = view.summary;

  if (summary.kind === 'requirements') {
    if (summary.items.length === 0) return <EmptyState kind="requirements" />;
    return (
      <Box flexDirection="column">
        {summary.items.map((item, i) => (
          <RequirementSummaryItem
            key={`${item.number}-${i}`}
            item={item}
            selected={view.cursor === i}
          />
        ))}
      </Box>
    );
  }

  if (summary.kind === 'design') {
    if (summary.sections.length === 0) return <EmptyState kind="design" />;
    return (
      <Box flexDirection="column">
        {summary.overview.length > 0 && (
          <Box marginBottom={1} flexDirection="column">
            <Text>{dim('Overview')}</Text>
            <Text>{summary.overview}</Text>
          </Box>
        )}
        {summary.sections.map((section, i) => (
          <DesignSummaryItem
            key={`${section.title}-${i}`}
            section={section}
            selected={view.cursor === i}
          />
        ))}
      </Box>
    );
  }

  // tasks
  if (summary.items.length === 0) return <EmptyState kind="tasks" />;
  return (
    <Box flexDirection="column">
      {summary.items.map((task, i) => (
        <TaskSummaryItem
          key={`${task.number || 'task'}-${i}`}
          task={task}
          selected={view.cursor === i}
          expanded={!!view.expanded[i]}
        />
      ))}
    </Box>
  );
};
