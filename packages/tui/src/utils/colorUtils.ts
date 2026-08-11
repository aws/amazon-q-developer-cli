import { chalk } from './color.js';
import type { ChalkColorName, TerminalColor } from '../types/themeTypes.js';
import type { StatusType } from '../types/componentTypes.js';

// Named color to hex conversion
// Named color to hex conversion
const namedColorToHex: { [key in ChalkColorName]: string } = {
  default: 'inherit', // Special marker for terminal default color
  black: '#000000',
  red: '#ff0000',
  green: '#00ff00',
  yellow: '#ffff00',
  blue: '#0000ff',
  magenta: '#ff00ff',
  cyan: '#00ffff',
  white: '#ffffff',
  blackBright: '#808080',
  redBright: '#ff8080',
  greenBright: '#80ff80',
  yellowBright: '#ffff80',
  blueBright: '#8080ff',
  magentaBright: '#ff80ff',
  cyanBright: '#80ffff',
  whiteBright: '#ffffff',
  gray: '#808080',
  grey: '#808080',
};

/**
 * Creates a chalk chain instance based on terminal color support capabilities.
 * Returns the chalk chain with an added .hex property containing the resolved color value.
 *
 * @param color - A TerminalColor object with optional truecolor, color256, and named fields
 * @param mode - 'fg' for foreground (default), 'bg' for background
 * @returns A chalk chain instance with .hex property
 */
export const getTerminalChalkColor = (
  color: TerminalColor,
  mode: 'fg' | 'bg' = 'fg'
): any => {
  const { truecolor, color256, named } = color;
  let chalkFunction: any;
  let resolvedHex: string = '#000000'; // Default fallback

  // Special case: 'default' means use terminal's default color.
  // We avoid chalk.reset here because it emits \x1b[0m which resets ALL
  // formatting (bold, italic, etc.). Instead, use a plain chalk instance
  // with no color applied — it supports .bold/.italic chaining without
  // emitting color reset codes.
  if (named === 'default') {
    const colorWrapper = (text: string) => text;
    colorWrapper.hex = 'inherit';
    Object.setPrototypeOf(colorWrapper, chalk);
    return colorWrapper;
  }

  // If no color values provided, return base chalk
  if (!truecolor && color256 === undefined && !named) {
    const colorWrapper = (text: string) => chalk(text);
    colorWrapper.hex = resolvedHex;
    return colorWrapper;
  }

  // Helper to pick fg or bg chalk method
  const hexFn = mode === 'bg' ? chalk.bgHex.bind(chalk) : chalk.hex.bind(chalk);
  const ansi256Fn =
    mode === 'bg' ? chalk.bgAnsi256.bind(chalk) : chalk.ansi256.bind(chalk);
  const namedFn = (name: ChalkColorName) => {
    const key =
      mode === 'bg' ? `bg${name[0]!.toUpperCase()}${name.slice(1)}` : name;
    return (chalk as any)[key] || chalk;
  };

  // Capability follows the shared instance's resolved color level.
  const has16m = chalk.level >= 3;
  const has256 = chalk.level >= 2;
  const hasColor = chalk.level >= 1;

  if (has16m) {
    // Truecolor terminal - use truecolor hex
    resolvedHex = truecolor || (named && namedColorToHex[named]) || '#000000';
  } else if (has256) {
    // 256-color terminal - pass ansi256(N) format to preserve the original
    // color index and avoid double-conversion through hex approximation
    resolvedHex =
      (color256 !== undefined && `ansi256(${color256})`) ||
      truecolor ||
      (named && namedColorToHex[named]) ||
      '#000000';
  } else {
    // Basic terminal - use named color hex
    resolvedHex = (named && namedColorToHex[named]) || truecolor || '#000000';
  }

  // Create chalk function based on terminal capabilities

  // Prefer truecolor (16 million colors) if terminal supports it
  if (has16m) {
    if (truecolor) {
      chalkFunction = hexFn(truecolor);
    } else if (color256 !== undefined) {
      chalkFunction = ansi256Fn(color256);
    } else if (named) {
      chalkFunction = namedFn(named);
    }
  }
  // Fall back to 256-color mode if supported
  else if (has256) {
    if (color256 !== undefined) {
      chalkFunction = ansi256Fn(color256);
    } else if (truecolor) {
      chalkFunction = hexFn(truecolor);
    } else if (named) {
      chalkFunction = namedFn(named);
    }
  }
  // Fall back to named colors if available
  else if (hasColor && named) {
    chalkFunction = namedFn(named);
  }
  // Final fallback
  else {
    if (truecolor) {
      chalkFunction = hexFn(truecolor);
    } else if (color256 !== undefined) {
      chalkFunction = ansi256Fn(color256);
    } else if (named) {
      chalkFunction = namedFn(named);
    } else {
      chalkFunction = chalk;
    }
  }

  // Create a wrapper that combines the chalk function with hex property
  const colorWrapper = (text: string) => {
    return chalkFunction ? chalkFunction(text) : chalk(text);
  };

  // Add the hex property to our wrapper
  colorWrapper.hex = resolvedHex;

  // Copy over any other chalk methods that might be needed
  Object.setPrototypeOf(colorWrapper, chalkFunction || chalk);

  return colorWrapper;
};

/**
 * Extracts the hex color value from a color function with fallback.
 *
 * @param colorFunc - A color function with a .hex property
 * @param fallbackHex - Optional fallback hex color (defaults to '#ffffff')
 * @returns The hex color string
 */
/**
 * Hex string for a theme color path, with a guard against the renderer's
 * black/inherit placeholders. Named-color (safe-mode) themes carry no hex —
 * the fallback fires there.
 */
export const themeHex = (
  getColor: (colorPath: string) => unknown,
  colorPath: string,
  fallback: string
): string => {
  const hex = (getColor(colorPath) as { hex?: string })?.hex;
  return hex && hex !== '#000000' && hex !== 'inherit' ? hex : fallback;
};

export const getColorHex = (
  colorFunc: any,
  fallbackHex: string = '#ffffff'
): string => {
  return colorFunc?.hex || fallbackHex;
};

/**
 * Maps a StatusType to its corresponding theme color.
 * Used for consistent status color handling across components.
 *
 * @param statusType - The status type ('success', 'error', 'warning', 'info', 'active')
 * @param getColor - The getColor function from useTheme hook
 * @returns The chalk color function for the status
 */
export const getStatusColor = (
  statusType: StatusType,
  getColor: (colorPath: string) => any
) => {
  switch (statusType) {
    case 'active':
      return getColor('brand');
    case 'success':
      return getColor('success');
    case 'info':
      return getColor('info');
    case 'warning':
      return getColor('warning');
    case 'error':
      return getColor('error');
    case 'loading':
      return getColor('secondary');
    case 'thinking':
      return getColor('brand');
    case 'executing':
      return getColor('brand');
    case 'paused':
      return getColor('secondary');
    default:
      return getColor('brand');
  }
};
