import { getTerminalChalkColor } from '../utils/colorUtils.js';
import type { ChalkColorName } from '../types/themeTypes.js';

export const useColor = (truecolor?: string, color256?: number, named?: ChalkColorName) => {
  return getTerminalChalkColor(truecolor, color256, named);
};
