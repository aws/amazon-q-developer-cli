import React from 'react';
import path from 'path';
import { Text } from 'ink';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface FileChipProps {
  filePath: string;
  lineCount: number;
}

export const FileChip = React.memo(function FileChip({
  filePath,
  lineCount,
}: FileChipProps) {
  const fileName = path.basename(filePath);
  const { getColor } = useTheme();
  return (
    <Text backgroundColor={getColor('muted').hex} color={getColor('brand').hex}>
      {` ${fileName}  ${lineCount} lines `}
    </Text>
  );
});
