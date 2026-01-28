import { Box } from 'ink';
import React, { useMemo } from 'react';
import { Text } from '../ui/text/Text.js';
import { StatusInfo } from '../ui/status/StatusInfo.js';

export interface ReadToolMessageProps {
  content: string;
  isFinished?: boolean;
}

interface ReadOp {
  path: string;
  limit?: number;
  offset?: number;
}

/**
 * Renders a human-readable description of a read tool operation.
 * 
 * Expects agent crate format: { ops: [{ path, limit?, offset? }] }
 */
export const ReadToolMessage: React.FC<ReadToolMessageProps> = ({ content, isFinished = false }) => {
  const ops = useMemo((): ReadOp[] => {
    try {
      const parsed = JSON.parse(content);
      
      if (parsed.ops && Array.isArray(parsed.ops)) {
        return parsed.ops.map((op: { path?: string; limit?: number; offset?: number }): ReadOp => ({
          path: op.path || '',
          limit: op.limit,
          offset: op.offset,
        }));
      }
      
      return [];
    } catch {
      return [];
    }
  }, [content]);

  const title = isFinished ? 'Read' : 'Reading';

  if (ops.length === 0) {
    return <StatusInfo title={title} />;
  }

  if (ops.length === 1) {
    const op = ops[0];
    return <StatusInfo title={title} target={op?.path || 'file'} />;
  }

  // Multiple files
  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={`${ops.length} files`} />
      {ops.map((op, i) => (
        <Box key={i} marginLeft={2}>
          <Text>↱ {op.path}</Text>
        </Box>
      ))}
    </Box>
  );
};
