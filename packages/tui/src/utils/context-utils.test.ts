import { describe, test, expect } from 'bun:test';

import { getUsageColor } from './context-utils';
import { ChipColor } from '../components/ui/chip/Chip.js';

describe('getUsageColor', () => {
  test('returns SUCCESS below 50%', () => {
    expect(getUsageColor(0)).toBe(ChipColor.SUCCESS);
    expect(getUsageColor(49)).toBe(ChipColor.SUCCESS);
  });

  test('returns WARNING between 50% and 75%', () => {
    expect(getUsageColor(50)).toBe(ChipColor.WARNING);
    expect(getUsageColor(74)).toBe(ChipColor.WARNING);
  });

  test('returns ERROR at 75% and above', () => {
    expect(getUsageColor(75)).toBe(ChipColor.ERROR);
    expect(getUsageColor(100)).toBe(ChipColor.ERROR);
  });
});
