import {
  createContext,
  useMemo,
  useState,
  useCallback,
  type ReactNode,
} from 'react';
import { kiroDark } from './kiroDark';
import { kiroLight } from './kiroLight';
import { kiroSafe } from './kiroSafe';
import type { Theme } from './types';
import type { TerminalColor } from '../types/themeTypes';
import { getTerminalChalkColor } from '../utils/colorUtils';
import { detectTerminalThemeWithDetails } from '../utils/terminal-theme';
import { logger } from '../utils/logger.js';
import {
  loadUserThemePrefs,
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
  type DiffPreset,
} from './user-theme';

/**
 * Extended theme context value that includes a helper function for getting terminal colors.
 * This interface combines the base Theme with a convenient color getter method.
 */
export interface ThemeContextValue extends Theme {
  getColor: (colorPath: string) => any; // Returns chalk chain that can be called or further chained
  /** User-customized prompt text color (falls back to primary) */
  getUserPromptColor: () => any;
  /** User-customized prompt background hex (falls back to surface, undefined if no bg) */
  getUserPromptBgHex: () => string | undefined;
  /** User-customized response text color (falls back to primary) */
  getUserResponseColor: () => any;
  /** Update user color overrides at runtime (triggers re-render).
   *  Pass null to clear an override, undefined to leave unchanged. */
  setUserColors: (
    prompt?: { text: TerminalColor; bg: TerminalColor } | null,
    response?: TerminalColor | null,
    diff?: DiffPreset | null
  ) => void;
  /** Switch the base theme at runtime. Pass null to reset to auto-detected. */
  setBaseTheme: (theme: Theme | null) => void;
  /** The raw base theme before user overrides (for Auto preview) */
  baseTheme: Theme;
  /**
   * When true, content in scrollback and live area renders with overflow
   * wrapping (no soft newlines in copy-paste) and without the StatusBar
   * left-bar chrome. Controlled by the `chat.disableWrap` setting (or the
   * `KIRO_DISABLE_WRAP=1` env var for dev).
   */
  wrapDisabled: boolean;
  /**
   * When true, the TUI is in lite mode (scrollback-friendly, no panels).
   */
  liteMode: boolean;
}

/**
 * Creates a theme context value with enhanced color functionality.
 * Transforms a basic Theme into a ThemeContextValue by adding the getColor helper method.
 *
 * @param theme - The base theme configuration
 * @param userPromptColor - Optional user override for prompt text color
 * @param userResponseColor - Optional user override for response text color
 * @param setUserColors - Callback to update user colors at runtime
 * @param wrapDisabled - Whether the TUI is rendering with soft-wrap overflow instead of word-wrap
 * @returns Enhanced theme context with color getter functionality
 */
/** @internal Exported for testing */
export const createThemeContext = (
  theme: Theme,
  userPromptColor: TerminalColor | undefined,
  userPromptBgColor: TerminalColor | undefined,
  userResponseColor: TerminalColor | undefined,
  userDiffPreset: DiffPreset | undefined,
  setUserColors: (
    prompt?: { text: TerminalColor; bg: TerminalColor } | null,
    response?: TerminalColor | null,
    diff?: DiffPreset | null
  ) => void,
  setBaseTheme: (theme: Theme | null) => void,
  wrapDisabled: boolean,
  liteMode: boolean = false
): ThemeContextValue => {
  // Merge user diff overrides into theme colors so getColor('diff.*') picks them up
  const effectiveColors =
    userDiffPreset && userDiffPreset.added.bar.named !== 'default'
      ? {
          ...theme.colors,
          diff: {
            added: {
              background: userDiffPreset.added.background,
              bar: userDiffPreset.added.bar,
              highlight: userDiffPreset.added.highlight,
              foreground: theme.colors.diff.added.foreground,
            },
            removed: {
              background: userDiffPreset.removed.background,
              bar: userDiffPreset.removed.bar,
              highlight: userDiffPreset.removed.highlight,
              foreground: theme.colors.diff.removed.foreground,
            },
            unchanged: theme.colors.diff.unchanged,
          },
        }
      : theme.colors;

  return {
    ...theme,
    colors: effectiveColors,
    getColor: (colorPath: string) => {
      const keys = colorPath.split('.');
      let colorDef: any = effectiveColors;

      for (const key of keys) {
        colorDef = colorDef[key];
        if (!colorDef) {
          throw new Error(`Color path '${colorPath}' not found in theme`);
        }
      }

      return getTerminalChalkColor(colorDef);
    },
    getUserPromptColor: () => {
      if (!userPromptColor) {
        return getTerminalChalkColor(theme.colors.primary);
      }
      return getTerminalChalkColor(userPromptColor);
    },
    getUserPromptBgHex: () => {
      if (userPromptBgColor?.truecolor) return userPromptBgColor.truecolor;
      // Fall back to theme surface; guard against 'inherit' from named:'default'
      const surfaceHex = getTerminalChalkColor(theme.colors.surface).hex;
      return surfaceHex === 'inherit' ? undefined : surfaceHex;
    },
    getUserResponseColor: () => {
      if (!userResponseColor) {
        return getTerminalChalkColor(theme.colors.primary);
      }
      return getTerminalChalkColor(userResponseColor);
    },
    setUserColors,
    setBaseTheme,
    baseTheme: theme,
    wrapDisabled,
    liteMode,
  };
};

/**
 * Gets the appropriate theme based on terminal/OS appearance detection.
 * Falls back to kiroSafe (ANSI named colors) when detection confidence is low
 * and no definitive light/dark signal is available — e.g., SSH into headless Linux.
 */
/** @internal Exported for testing */
export const getAutoTheme = (): Theme => {
  // Deterministic override: forces the base theme and skips detection
  // entirely (including the OSC-11 tty query). Used by E2E tests to pin a
  // theme path, and an escape hatch for terminals that defeat detection.
  const forced = process.env.KIRO_TERMINAL_THEME?.toLowerCase();
  if (forced === 'dark' || forced === 'light' || forced === 'safe') {
    logger.info(`[theme] forced=${forced} via KIRO_TERMINAL_THEME`);
    return forced === 'dark'
      ? kiroDark
      : forced === 'light'
        ? kiroLight
        : kiroSafe;
  }

  const result = detectTerminalThemeWithDetails();
  // Debugging theme issues (e.g. unreadable diff colors) needs to know which
  // detection path fired — it's invisible from the UI otherwise.
  logger.info(
    `[theme] detected=${result.theme} method=${result.method} confidence=${result.confidence}`
  );

  // High/medium confidence: we know the actual background, pick accordingly
  if (result.confidence === 'high' || result.confidence === 'medium') {
    return result.theme === 'light' ? kiroLight : kiroDark;
  }

  // Low confidence: use the safe ANSI theme that works on both light and dark
  // backgrounds. This covers SSH sessions to headless servers, terminals where
  // OSC 11 fails, and cases where gsettings/env vars give unreliable signals.
  return kiroSafe;
};

// Create the React context with the default theme as the initial value
export const ThemeContext = createContext<ThemeContextValue>(
  createThemeContext(
    kiroDark,
    undefined,
    undefined,
    undefined,
    undefined,
    () => {},
    () => {},
    false,
    false
  )
);

/**
 * Props for the ThemeProvider component
 */
interface ThemeProviderProps {
  theme?: Theme | 'auto'; // Optional theme override, 'auto' for detection, defaults to auto-detection
  /**
   * When true, the TUI renders with `wrap="overflow"` (long lines soft-wrap
   * visually but stay as single logical lines) and the StatusBar chrome is
   * dropped. Controlled by the `chat.disableWrap` setting at startup.
   */
  wrapDisabled?: boolean;
  /** When true, the TUI is in lite mode. */
  liteMode?: boolean;
  children: ReactNode;
}

/**
 * ThemeProvider component that provides theme context to child components.
 * Wraps children with theme context, allowing them to access theme values and color utilities.
 * Loads user color preferences from ~/.kiro/settings/kiro_cli_theme.json on mount.
 *
 * @param theme - Optional theme override ('auto' for detection, or a specific Theme). Defaults to 'auto'.
 * @param children - Child components that will have access to the theme context
 */
export const ThemeProvider = ({
  theme = 'auto',
  wrapDisabled = false,
  liteMode = false,
  children,
}: ThemeProviderProps) => {
  // Detect theme once on mount — avoid re-running OSC 11 queries on every render
  // which can corrupt terminal state (especially over SSH).
  const autoTheme = useMemo(
    () => (theme === 'auto' ? getAutoTheme() : theme),
    [theme]
  );

  // Load persisted user color prefs on mount
  const initialPrefs = useMemo(() => loadUserThemePrefs(), []);

  // Base theme override: null = use auto-detected, Theme = user-selected
  const [baseThemeOverride, setBaseThemeOverride] = useState<Theme | null>(
    () => {
      if (initialPrefs.baseTheme === 'dark') return kiroDark;
      if (initialPrefs.baseTheme === 'light') return kiroLight;
      return null;
    }
  );

  const resolvedTheme = baseThemeOverride ?? autoTheme;

  const setBaseTheme = useCallback((newTheme: Theme | null) => {
    setBaseThemeOverride(newTheme);
  }, []);

  const [userPromptColor, setUserPromptColor] = useState<
    TerminalColor | undefined
  >(() => getPromptPreset(initialPrefs.promptPreset)?.textColor);
  const [userPromptBgColor, setUserPromptBgColor] = useState<
    TerminalColor | undefined
  >(() => getPromptPreset(initialPrefs.promptPreset)?.bgColor);
  const [userResponseColor, setUserResponseColor] = useState<
    TerminalColor | undefined
  >(() => getResponsePreset(initialPrefs.responsePreset)?.textColor);
  const [userDiffPreset, setUserDiffPreset] = useState<DiffPreset | undefined>(
    () => getDiffPreset(initialPrefs.diffPreset)
  );

  const setUserColors = useCallback(
    (
      prompt?: { text: TerminalColor; bg: TerminalColor } | null,
      response?: TerminalColor | null,
      diff?: DiffPreset | null
    ) => {
      // null = clear override, undefined = don't change
      if (prompt === null) {
        setUserPromptColor(undefined);
        setUserPromptBgColor(undefined);
      } else if (prompt !== undefined) {
        setUserPromptColor(prompt.text);
        setUserPromptBgColor(prompt.bg);
      }
      if (response === null) {
        setUserResponseColor(undefined);
      } else if (response !== undefined) {
        setUserResponseColor(response);
      }
      if (diff === null) {
        setUserDiffPreset(undefined);
      } else if (diff !== undefined) {
        setUserDiffPreset(diff);
      }
    },
    []
  );

  const themeContext = useMemo(
    () =>
      createThemeContext(
        resolvedTheme,
        userPromptColor,
        userPromptBgColor,
        userResponseColor,
        userDiffPreset,
        setUserColors,
        setBaseTheme,
        wrapDisabled,
        liteMode
      ),
    [
      resolvedTheme,
      userPromptColor,
      userPromptBgColor,
      userResponseColor,
      userDiffPreset,
      setUserColors,
      setBaseTheme,
      wrapDisabled,
      liteMode,
    ]
  );
  return (
    <ThemeContext.Provider value={themeContext}>
      {children}
    </ThemeContext.Provider>
  );
};
