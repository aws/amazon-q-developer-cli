import { execSync } from 'child_process';
import { readFileSync } from 'fs';
import { join } from 'path';

interface DetectionResult {
  theme: 'dark' | 'light';
  method: string;
  confidence: 'high' | 'medium' | 'low';
}

interface WTColorScheme {
  name: string;
  background?: string;
}

interface WTProfile {
  colorScheme?: string;
  guid?: string;
}

interface WTSettings {
  profiles?: {
    defaults?: WTProfile;
    list?: WTProfile[];
  };
  schemes?: WTColorScheme[];
}

/**
 * Detects theme from Windows Terminal's settings.json by reading the active
 * color scheme's background color and computing its luminance.
 *
 * Windows Terminal stores settings at:
 * - Store install: %LOCALAPPDATA%\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json
 * - Scoop/portable: %LOCALAPPDATA%\Microsoft\Windows Terminal\settings.json
 *
 * The detection reads the default profile's colorScheme, looks it up in the
 * schemes array, and determines dark/light from the background color.
 */
export function detectWindowsTerminalTheme(): DetectionResult | null {
  if (process.platform !== 'win32') {
    return null;
  }

  const localAppData = process.env.LOCALAPPDATA;
  if (!localAppData) {
    return null;
  }

  // Try both known settings.json locations
  const settingsPaths = [
    join(
      localAppData,
      'Packages',
      'Microsoft.WindowsTerminal_8wekyb3d8bbwe',
      'LocalState',
      'settings.json'
    ),
    join(localAppData, 'Microsoft', 'Windows Terminal', 'settings.json'),
  ];

  for (const settingsPath of settingsPaths) {
    const result = tryParseWTSettings(settingsPath);
    if (result) {
      return result;
    }
  }

  return null;
}

function tryParseWTSettings(settingsPath: string): DetectionResult | null {
  try {
    const raw = readFileSync(settingsPath, 'utf8');
    // Strip JSON comments (// and /* */) that Windows Terminal allows
    const stripped = raw
      .replace(/\/\/.*$/gm, '')
      .replace(/\/\*[\s\S]*?\*\//g, '');
    const settings: WTSettings = JSON.parse(stripped);

    // Get the default color scheme name
    const defaultScheme =
      settings.profiles?.defaults?.colorScheme ?? 'Campbell';

    // Find the scheme definition
    const scheme = settings.schemes?.find(
      (s) => s.name.toLowerCase() === defaultScheme.toLowerCase()
    );

    if (!scheme?.background) {
      // "Campbell" and other built-in schemes may not be in the user's
      // settings.json. Use known built-in scheme backgrounds.
      return detectFromBuiltinScheme(defaultScheme);
    }

    const theme = hexToTheme(scheme.background);
    if (theme) {
      return {
        theme,
        method: `WT-settings(${defaultScheme})`,
        confidence: 'medium',
      };
    }
  } catch {
    // File not found, parse error, or permission denied
  }

  return null;
}

/**
 * Determines dark/light from a hex color string like "#1e1e1e" or "#ffffff".
 */
function hexToTheme(hex: string): 'dark' | 'light' | null {
  const match = hex.match(/^#?([0-9a-fA-F]{6})$/);
  if (!match?.[1]) {
    return null;
  }

  const r = parseInt(match[1].substring(0, 2), 16);
  const g = parseInt(match[1].substring(2, 4), 16);
  const b = parseInt(match[1].substring(4, 6), 16);

  // Same luminance formula as osc-query.ts
  const luminance = 0.299 * r + 0.587 * g + 0.114 * b;
  return luminance > 128 ? 'light' : 'dark';
}

/**
 * Maps well-known Windows Terminal built-in color scheme names to dark/light.
 * These schemes are compiled into WT and won't appear in the user's settings.json.
 */
function detectFromBuiltinScheme(schemeName: string): DetectionResult | null {
  const name = schemeName.toLowerCase();

  // Built-in dark schemes
  const darkSchemes = [
    'campbell',
    'campbell powershell',
    'one half dark',
    'tango dark',
    'vintage',
  ];

  // Built-in light schemes
  const lightSchemes = ['one half light', 'tango light'];

  if (darkSchemes.includes(name)) {
    return {
      theme: 'dark',
      method: `WT-builtin(${schemeName})`,
      confidence: 'medium',
    };
  }

  if (lightSchemes.includes(name)) {
    return {
      theme: 'light',
      method: `WT-builtin(${schemeName})`,
      confidence: 'medium',
    };
  }

  // Unknown scheme name — try keyword matching as last resort
  if (name.includes('light')) {
    return {
      theme: 'light',
      method: `WT-schemeName(${schemeName})`,
      confidence: 'low',
    };
  }
  if (name.includes('dark')) {
    return {
      theme: 'dark',
      method: `WT-schemeName(${schemeName})`,
      confidence: 'low',
    };
  }

  return null;
}

/**
 * Detects the console background color on Windows by querying PowerShell's
 * $Host.UI.RawUI.BackgroundColor. This works for PowerShell and cmd.exe
 * consoles that aren't running inside Windows Terminal.
 */
export function detectWindowsConsoleBackground(): DetectionResult | null {
  try {
    const output = execSync(
      'powershell -NoProfile -Command "$Host.UI.RawUI.BackgroundColor"',
      {
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'ignore'],
        timeout: 2000,
      }
    )
      .trim()
      .toLowerCase();

    const darkColors = [
      'black',
      'darkblue',
      'darkgreen',
      'darkcyan',
      'darkred',
      'darkmagenta',
      'darkyellow',
      'darkgray',
    ];
    const lightColors = [
      'gray',
      'blue',
      'green',
      'cyan',
      'red',
      'magenta',
      'yellow',
      'white',
    ];

    if (darkColors.includes(output)) {
      return {
        theme: 'dark',
        method: 'Win-ConsoleBackground',
        confidence: 'medium',
      };
    }
    if (lightColors.includes(output)) {
      return {
        theme: 'light',
        method: 'Win-ConsoleBackground',
        confidence: 'medium',
      };
    }
  } catch {
    // PowerShell not available or command failed
  }

  return null;
}
