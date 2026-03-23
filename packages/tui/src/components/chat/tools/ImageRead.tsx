import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import type { StatusType } from '../../../types/componentTypes.js';
import { getToolLabel } from '../../../types/tool-status.js';

export interface ImageReadProps {
  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the tool operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;
}

/**
 * ImageRead tool component — displays only the tool status and target paths,
 * with no output section (image data is not useful to render in the terminal).
 */
export const ImageRead = React.memo(function ImageRead({
  status,
  noStatusBar = false,
  isFinished = false,
  content,
}: ImageReadProps) {
  const title = getToolLabel('image_read');

  const params = useMemo(() => formatToolParams(content, ['paths']), [content]);

  // Extract a display target from the paths arg
  const target = useMemo(() => {
    if (!content) return undefined;
    try {
      const parsed = JSON.parse(content);
      const paths: string[] = parsed.paths ?? [];
      if (paths.length === 0) return undefined;
      if (paths.length === 1) {
        return paths[0]!.split('/').pop() || paths[0];
      }
      return `(${paths.length} images)`;
    } catch {
      return undefined;
    }
  }, [content]);

  const displayContent = (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
    </Box>
  );

  if (noStatusBar) return displayContent;
  return <StatusBar status={status}>{displayContent}</StatusBar>;
});
