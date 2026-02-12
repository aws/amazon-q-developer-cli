import chalk from 'chalk';
import supportsColor from 'supports-color';
import type { ChalkColorName } from '../types/themeTypes.js';
import type { StatusType } from '../types/componentTypes.js';

// Named color to hex conversion
const namedColorToHex: { [key in ChalkColorName]: string } = {
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

// Convert ANSI 256 color to hex
const color256ToHex = (color: number): string => {
  if (color < 16) {
    // Standard colors (0-15)
    const standardColors = [
      '#000000',
      '#800000',
      '#008000',
      '#808000',
      '#000080',
      '#800080',
      '#008080',
      '#c0c0c0',
      '#808080',
      '#ff0000',
      '#00ff00',
      '#ffff00',
      '#0000ff',
      '#ff00ff',
      '#00ffff',
      '#ffffff',
    ];
    return standardColors[color] || '#000000';
  } else if (color < 232) {
    // 216 color cube (16-231)
    const index = color - 16;
    const r = Math.floor(index / 36);
    const g = Math.floor((index % 36) / 6);
    const b = index % 6;

    const toHex = (val: number) => {
      const intensity = val === 0 ? 0 : 55 + val * 40;
      return intensity.toString(16).padStart(2, '0');
    };

    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  } else {
    // Grayscale (232-255)
    const gray = 8 + (color - 232) * 10;
    const hex = gray.toString(16).padStart(2, '0');
    return `#${hex}${hex}${hex}`;
  }
};

/**
 * Creates a chalk chain instance based on terminal color support capabilities.
 * Returns the chalk chain with an added .hex property containing the resolved color value.
 *
 * @param truecolor - Optional hex color string (e.g., "#ff0000")
 * @param color256 - Optional ANSI 256 color index (0-255)
 * @param named - Optional named color (e.g., "magenta", "red", "blue")
 * @returns A chalk chain instance with .hex property
 */
export const getTerminalChalkColor = (
  truecolor?: string,
  color256?: number,
  named?: ChalkColorName,
): any => {
  let chalkFunction: any;
  let resolvedHex: string = '#000000'; // Default fallback

  // If no color values provided, return base chalk
  if (!truecolor && !color256 && !named) {
    const colorWrapper = (text: string) => chalk(text);
    colorWrapper.hex = resolvedHex;
    return colorWrapper;
  }

  // For hex value, use the appropriate color based on terminal capabilities
  const stdout = supportsColor.stdout;

  if (stdout && typeof stdout === 'object' && 'has16m' in stdout && stdout.has16m) {
    // Truecolor terminal - use truecolor hex
    resolvedHex = truecolor || (named && namedColorToHex[named]) || '#000000';
  } else if (stdout && typeof stdout === 'object' && 'has256' in stdout && stdout.has256) {
    // 256-color terminal - use color256 hex equivalent
    resolvedHex =
      (color256 && color256ToHex(color256)) ||
      truecolor ||
      (named && namedColorToHex[named]) ||
      '#000000';
  } else {
    // Basic terminal - use named color hex
    resolvedHex = (named && namedColorToHex[named]) || truecolor || '#000000';
  }

  // Create chalk function based on terminal capabilities

  // Prefer truecolor (16 million colors) if terminal supports it
  if (stdout && typeof stdout === 'object' && 'has16m' in stdout && stdout.has16m) {
    if (truecolor) {
      chalkFunction = chalk.hex(truecolor);
    } else if (color256) {
      chalkFunction = chalk.ansi256(color256);
    } else if (named) {
      chalkFunction = (chalk as any)[named] || chalk;
    }
  }
  // Fall back to 256-color mode if supported
  else if (stdout && typeof stdout === 'object' && 'has256' in stdout && stdout.has256) {
    if (color256) {
      chalkFunction = chalk.ansi256(color256);
    } else if (truecolor) {
      chalkFunction = chalk.hex(truecolor);
    } else if (named) {
      chalkFunction = (chalk as any)[named] || chalk;
    }
  }
  // Fall back to named colors if available
  else if (stdout && named) {
    chalkFunction = (chalk as any)[named] || chalk;
  }
  // Final fallback
  else {
    if (truecolor) {
      chalkFunction = chalk.hex(truecolor);
    } else if (color256) {
      chalkFunction = chalk.ansi256(color256);
    } else if (named) {
      chalkFunction = (chalk as any)[named] || chalk;
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
export const getColorHex = (colorFunc: any, fallbackHex: string = '#ffffff'): string => {
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
  getColor: (colorPath: string) => any,
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
    default:
      return getColor('brand');
  }
};
