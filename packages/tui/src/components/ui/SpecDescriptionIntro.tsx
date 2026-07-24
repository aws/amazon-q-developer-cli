import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { StatusBar } from '../chat/status-bar/StatusBar.js';

/**
 * Intro block for the `/spec new` description-collection step. Rendered in
 * the live region (never the transcript) while the step is armed, so it
 * disappears on its own when the user submits a description or cancels —
 * nothing is written to scrollback for a step that may not happen.
 * Styled like a system message so it reads as part of the conversation.
 */
export const SpecDescriptionIntro: React.FC<{ featureName: string }> = ({
  featureName,
}) => (
  <Box marginTop={1}>
    <StatusBar status="success">
      <Text>
        {`Starting spec: "${featureName}"\n\nWhat should this spec cover? Describe it in a sentence or two.`}
      </Text>
    </StatusBar>
  </Box>
);
