import React from 'react';
import path from 'path';
import Chip, { ChipColor } from '../../ui/chip/Chip.js';

export interface FileChipProps {
  filePath: string;
  lineCount: number;
}

export const FileChip = React.memo(function FileChip({ filePath, lineCount }: FileChipProps) {
  const fileName = path.basename(filePath);
  return (
    <Chip
      value={` ${fileName}  ${lineCount} lines `}
      color={ChipColor.BRAND}
      background={true}
    />
  );
});
