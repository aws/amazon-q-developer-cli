import React from 'react';
import Chip, { ChipColor } from '../../ui/chip/Chip.js';

export const PASTE_COLLAPSE_THRESHOLD_LINES = 10;
export const PASTE_COLLAPSE_THRESHOLD_CHARS = 500;

export type PastedContentType = 'text' | 'image';

export interface PastedChipProps {
  type?: PastedContentType;
  lineCount?: number;
  charCount?: number;
  // For future image support
  imageWidth?: number;
  imageHeight?: number;
  imageSizeBytes?: number;
}

export function shouldCollapsePaste(content: string): { shouldCollapse: boolean; lineCount: number; charCount: number } {
  // Count lines by splitting on any newline variant (\n, \r\n, or \r)
  const lines = content.split(/\r\n|\r|\n/);
  const lineCount = lines.length;
  const charCount = content.length;
  const shouldCollapse = lineCount > PASTE_COLLAPSE_THRESHOLD_LINES || charCount > PASTE_COLLAPSE_THRESHOLD_CHARS;
  return { shouldCollapse, lineCount, charCount };
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function PastedChip({ 
  type = 'text', 
  lineCount = 0, 
  charCount = 0,
  imageWidth,
  imageHeight,
  imageSizeBytes,
}: PastedChipProps) {
  let label: string;
  
  if (type === 'image') {
    // Format image info
    const dimensions = imageWidth && imageHeight ? `${imageWidth}×${imageHeight}` : '';
    const size = imageSizeBytes ? formatBytes(imageSizeBytes) : '';
    const details = [dimensions, size].filter(Boolean).join(' ');
    label = `pasted image${details ? ` (${details})` : ''}`;
  } else {
    // Text content
    label = lineCount > 1 
      ? `${lineCount} lines` 
      : `${charCount} chars`;
  }

  return (
    <Chip
      value={` ${label} `}
      color={ChipColor.BRAND}
      background={true}
    />
  );
}
