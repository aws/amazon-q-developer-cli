import { ChipColor } from '../components/ui/chip/Chip.js';

export const DEFAULT_CONTEXT_WINDOW = 200000;

export function getUsageColor(percent: number): ChipColor {
  if (percent < 50) return ChipColor.SUCCESS;
  if (percent < 75) return ChipColor.WARNING;
  return ChipColor.ERROR;
}
