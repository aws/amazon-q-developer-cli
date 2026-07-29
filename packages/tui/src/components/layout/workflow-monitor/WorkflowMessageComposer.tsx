import React from 'react';
import { Box, CURSOR_MARKER, Text } from '../../../renderer.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export type WorkflowMessageMode = 'steer' | 'respond' | 'message';

export const WORKFLOW_MESSAGE_COMPOSER_HEIGHT = 5;

const MODE_LABEL: Record<WorkflowMessageMode, string> = {
  steer: 'Steer',
  respond: 'Respond',
  message: 'Message',
};

export interface WorkflowMessageComposerProps {
  mode: WorkflowMessageMode;
  targetLabel: string;
  value: string;
  width: number;
}

export const WorkflowMessageComposer = React.memo(
  function WorkflowMessageComposer({
    mode,
    targetLabel,
    value,
    width,
  }: WorkflowMessageComposerProps) {
    const { getColor } = useTheme();
    const glyphs = useGlyphs();

    return (
      <Box
        flexDirection="column"
        width={Math.max(4, width - 2)}
        height={WORKFLOW_MESSAGE_COMPOSER_HEIGHT}
        marginX={1}
        paddingX={1}
        borderStyle="round"
        borderColor={getColor('brand').hex}
      >
        <Text wrap="truncate">
          {getColor('brand')(
            `${MODE_LABEL[mode]} ${glyphs.arrow} ${targetLabel}`
          )}
        </Text>
        <Text wrap="truncate">
          {value}
          {CURSOR_MARKER}
        </Text>
        <Text wrap="truncate">
          {getColor('secondary')(
            `${glyphs.enter} send ${glyphs.smallDot} ${glyphs.arrowUp}${glyphs.arrowDown} nodes ${glyphs.smallDot} esc close`
          )}
        </Text>
      </Box>
    );
  }
);
