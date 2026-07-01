import React from 'react';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';

interface GoalPanelProps {
  onClose: () => void;
}

export const GoalPanel: React.FC<GoalPanelProps> = ({ onClose }) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const goalStatus = useAppStore((state) => state.goalStatus);
  const dim = getColor('secondary');
  const primary = getColor('primary');

  if (!goalStatus) {
    return (
      <Panel title="Goal" onClose={onClose} footerLeft={dim('esc to close')}>
        <Text>
          {dim('No active goal. Use /goal <description> --max N to set one.')}
        </Text>
      </Panel>
    );
  }

  const stateIcon =
    goalStatus.state === 'completed'
      ? glyphs.checkmark
      : goalStatus.state === 'exhausted'
        ? glyphs.cross
        : goalStatus.state === 'paused'
          ? glyphs.pause
          : glyphs.executing;
  const stateLabel =
    goalStatus.state === 'completed'
      ? 'Completed'
      : goalStatus.state === 'exhausted'
        ? 'Exhausted'
        : goalStatus.state === 'paused'
          ? 'Paused'
          : 'Active';
  const stateColor =
    goalStatus.state === 'completed'
      ? 'success'
      : goalStatus.state === 'exhausted'
        ? 'error'
        : goalStatus.state === 'paused'
          ? 'warning'
          : 'primary';

  return (
    <Panel
      title="Goal"
      onClose={onClose}
      footerLeft={dim(`esc to close ${glyphs.smallDot} /goal clear to cancel`)}
    >
      <Text>
        {primary('Status: ')}
        {getColor(stateColor)(`${stateIcon} ${stateLabel}`)}
        {dim(` [${goalStatus.iteration + 1}/${goalStatus.maxIterations}]`)}
      </Text>
      {goalStatus.message && (
        <Text>
          {primary('Goal: ')}
          {goalStatus.message}
        </Text>
      )}
    </Panel>
  );
};
