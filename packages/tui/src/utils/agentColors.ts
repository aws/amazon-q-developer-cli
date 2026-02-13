import { kiroDark } from '../theme/kiroDark.js';
import { getTerminalChalkColor } from './colorUtils.js';

export const DEFAULT_AGENT_NAME = 'kiro_default';

// 20-color palette for agent names with 256-color fallbacks
// - Avoids red, white, yellow (warning), magenta/purple (brand), bright green (success)
const AGENT_COLORS: Array<{ truecolor: string; color256: number }> = [
  { truecolor: '#00d7d7', color256: 44 }, // cyan
  { truecolor: '#ff8700', color256: 208 }, // orange
  { truecolor: '#5f87ff', color256: 69 }, // blue
  { truecolor: '#00af87', color256: 36 }, // teal
  { truecolor: '#d78700', color256: 172 }, // dark orange
  { truecolor: '#5fafff', color256: 75 }, // sky blue
  { truecolor: '#00d7af', color256: 43 }, // aquamarine
  { truecolor: '#af8700', color256: 136 }, // gold
  { truecolor: '#00afff', color256: 39 }, // deep sky blue
  { truecolor: '#d75f00', color256: 166 }, // burnt orange
  { truecolor: '#5fd7ff', color256: 81 }, // light blue
  { truecolor: '#af5f00', color256: 130 }, // brown
  { truecolor: '#5fafd7', color256: 74 }, // steel blue
  { truecolor: '#d7af5f', color256: 179 }, // tan
  { truecolor: '#5f87d7', color256: 68 }, // cornflower
  { truecolor: '#87afff', color256: 111 }, // light steel blue
  { truecolor: '#00afd7', color256: 38 }, // dark cyan
  { truecolor: '#d7875f', color256: 173 }, // copper
  { truecolor: '#5fd7d7', color256: 80 }, // medium cyan
  { truecolor: '#af875f', color256: 137 }, // khaki
];

function hashString(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash * 31 + str.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

/**
 * Returns a chalk color function for the given agent name.
 * Uses brand color for default agent, otherwise hashes to a color from the palette.
 */
export function getAgentColor(
  name: string
): ReturnType<typeof getTerminalChalkColor> {
  if (name === DEFAULT_AGENT_NAME) {
    const brand = kiroDark.colors.brand;
    return getTerminalChalkColor(brand.truecolor, brand.color256);
  }
  const color = AGENT_COLORS[hashString(name) % AGENT_COLORS.length]!;
  return getTerminalChalkColor(color.truecolor, color.color256);
}
