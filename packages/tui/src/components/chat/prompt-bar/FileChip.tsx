import React from 'react';
import path from 'path';
import { PromptChip } from './PromptChip.js';

export interface FileChipProps {
  filePath: string;
  lineCount: number;
}

export const FileChip = React.memo(function FileChip({
  filePath,
  lineCount,
}: FileChipProps) {
  const fileName = path.basename(filePath);
  return <PromptChip label={`${fileName}  ${lineCount} lines`} />;
});
