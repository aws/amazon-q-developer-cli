import { execSync } from 'child_process';
import { getOSAppearance } from './os-appearance';

export type TerminalTheme = 'dark' | 'light';

interface DetectionResult {
  theme: TerminalTheme;
  method: string;
  confidence: 'high' | 'medium' | 'low';
}

/**
 * Detects terminal theme using multiple methods in order of reliability:
 * 1. COLORFGBG environment variable (terminal-specific, high confidence)
 * 2. Terminal-specific environment variables (medium confidence)
 * 3. OS appearance preference (low confidence - may not match terminal)
 * 4. Default to dark (fallback)
 */
export function detectTerminalTheme(): TerminalTheme {
  const result = detectTerminalThemeWithDetails();
  return result.theme;
}

/**
 * Detects terminal theme and returns details about the detection method.
 * Useful for debugging or logging.
 */
export function detectTerminalThemeWithDetails(): DetectionResult {
  // Method 1: COLORFGBG environment variable
  // Set by terminals like rxvt, xterm, and some others
  // Format: "foreground;background" or "foreground;background;cursor"
  // Common values: "15;0" (white on black = dark), "0;15" (black on white = light)
  const colorFgBg = process.env.COLORFGBG;
  if (colorFgBg) {
    const parts = colorFgBg.split(';');
    if (parts.length >= 2) {
      const bg = parseInt(parts[1] ?? '', 10);
      if (!isNaN(bg)) {
        // ANSI colors 0-7 are dark, 8-15 are bright
        // Background colors 0-6 are typically dark, 7+ are light
        // Special case: 0 = black (dark), 7 = white (light), 15 = bright white (light)
        const isDark = bg < 7 || bg === 8;
        return {
          theme: isDark ? 'dark' : 'light',
          method: 'COLORFGBG',
          confidence: 'high',
        };
      }
    }
  }

  // Method 2: Terminal-specific environment variables
  const terminalTheme = detectFromTerminalEnv();
  if (terminalTheme) {
    return terminalTheme;
  }

  // Method 3: OS appearance using existing detection (macOS/Windows)
  const osTheme = getOSAppearance();
  // Only trust OS detection on macOS/Windows where it's implemented
  if (process.platform === 'darwin' || process.platform === 'win32') {
    return {
      theme: osTheme,
      method:
        process.platform === 'darwin'
          ? 'macOS-AppleInterfaceStyle'
          : 'Windows-Registry',
      confidence: 'low',
    };
  }

  // Method 4: Linux-specific detection
  const linuxTheme = detectLinuxTheme();
  if (linuxTheme) {
    return linuxTheme;
  }

  // Method 5: Default to dark
  return {
    theme: 'dark',
    method: 'default',
    confidence: 'low',
  };
}

/**
 * Detect theme from terminal-specific environment variables
 */
function detectFromTerminalEnv(): DetectionResult | null {
  // Ghostty: Default to dark theme since most Ghostty users use dark themes
  // and Ghostty doesn't expose its theme via environment variables
  const isGhostty =
    process.env.GHOSTTY_RESOURCES_DIR || process.env.TERM_PROGRAM === 'ghostty';
  if (isGhostty) {
    return { theme: 'dark', method: 'Ghostty-default', confidence: 'medium' };
  }

  // iTerm2: Check ITERM_PROFILE for common naming patterns
  const itermProfile = process.env.ITERM_PROFILE?.toLowerCase();
  if (itermProfile) {
    if (itermProfile.includes('light')) {
      return { theme: 'light', method: 'ITERM_PROFILE', confidence: 'medium' };
    }
    if (itermProfile.includes('dark')) {
      return { theme: 'dark', method: 'ITERM_PROFILE', confidence: 'medium' };
    }
  }

  // Kitty: Check KITTY_THEME or kitty config hints
  const kittyTheme = process.env.KITTY_THEME?.toLowerCase();
  if (kittyTheme) {
    if (kittyTheme.includes('light')) {
      return { theme: 'light', method: 'KITTY_THEME', confidence: 'medium' };
    }
    if (kittyTheme.includes('dark')) {
      return { theme: 'dark', method: 'KITTY_THEME', confidence: 'medium' };
    }
  }

  // Windows Terminal: Check WT_SESSION (indicates Windows Terminal is in use)
  // Combined with OS theme detection for better accuracy
  if (process.env.WT_SESSION) {
    // Windows Terminal typically follows system theme
    // Fall through to OS detection with medium confidence
  }

  // VS Code integrated terminal
  const vscodeTerminal = process.env.TERM_PROGRAM === 'vscode';
  const vscodeTheme = process.env.VSCODE_TERMINAL_THEME?.toLowerCase();
  if (vscodeTerminal && vscodeTheme) {
    if (vscodeTheme.includes('light')) {
      return {
        theme: 'light',
        method: 'VSCODE_TERMINAL_THEME',
        confidence: 'medium',
      };
    }
    if (vscodeTheme.includes('dark')) {
      return {
        theme: 'dark',
        method: 'VSCODE_TERMINAL_THEME',
        confidence: 'medium',
      };
    }
  }

  // Hyper terminal
  const hyperTheme = process.env.HYPER_THEME?.toLowerCase();
  if (hyperTheme) {
    if (hyperTheme.includes('light')) {
      return { theme: 'light', method: 'HYPER_THEME', confidence: 'medium' };
    }
    if (hyperTheme.includes('dark')) {
      return { theme: 'dark', method: 'HYPER_THEME', confidence: 'medium' };
    }
  }

  return null;
}

/**
 * Detect theme on Linux using various desktop environment methods
 */
function detectLinuxTheme(): DetectionResult | null {
  // GNOME/GTK
  try {
    const gtkTheme = execSync(
      'gsettings get org.gnome.desktop.interface color-scheme',
      {
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'ignore'],
        timeout: 1000,
      }
    ).trim();

    if (gtkTheme.includes('dark')) {
      return { theme: 'dark', method: 'GNOME-color-scheme', confidence: 'low' };
    }
    if (gtkTheme.includes('light') || gtkTheme.includes('default')) {
      return {
        theme: 'light',
        method: 'GNOME-color-scheme',
        confidence: 'low',
      };
    }
  } catch {
    // gsettings not available or GNOME not in use
  }

  // KDE Plasma
  try {
    const kdeConfig = execSync(
      'kreadconfig5 --group General --key ColorScheme',
      {
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'ignore'],
        timeout: 1000,
      }
    )
      .trim()
      .toLowerCase();

    if (kdeConfig.includes('dark')) {
      return { theme: 'dark', method: 'KDE-ColorScheme', confidence: 'low' };
    }
    if (kdeConfig.includes('light') || kdeConfig.includes('breeze')) {
      return { theme: 'light', method: 'KDE-ColorScheme', confidence: 'low' };
    }
  } catch {
    // kreadconfig5 not available or KDE not in use
  }

  // Check GTK_THEME environment variable
  const gtkThemeEnv = process.env.GTK_THEME?.toLowerCase();
  if (gtkThemeEnv) {
    if (gtkThemeEnv.includes('dark')) {
      return { theme: 'dark', method: 'GTK_THEME', confidence: 'low' };
    }
    if (gtkThemeEnv.includes('light')) {
      return { theme: 'light', method: 'GTK_THEME', confidence: 'low' };
    }
  }

  return null;
}
