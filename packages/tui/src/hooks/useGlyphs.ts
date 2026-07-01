import React, { createContext, useContext, useState, useMemo } from 'react';
import {
  UNICODE_GLYPHS,
  ASCII_GLYPHS,
  UNICODE_SPINNERS,
  ASCII_SPINNERS,
  type Glyphs,
  type Spinners,
} from '../utils/glyphs.js';
import { readBoolSetting, readCliSettings } from '../utils/cli-settings.js';
import { Settings } from '../constants/settings.js';

/**
 * Display mode for the model's reasoning ("thinking") block.
 * - `collapsed`: header only by default; Ctrl+O expands the live stream.
 * - `expanded`: stream always shown (Ctrl+O is a noop on thinking).
 * - `off`: reasoning is never rendered.
 */
export type ThinkingMode = 'collapsed' | 'expanded' | 'off';

interface GlyphsContextValue {
  glyphs: Glyphs;
  spinners: Spinners;
  allowAsciiArt: boolean;
  setAllowAsciiArt: (v: boolean) => void;
  allowAnimations: boolean;
  setAllowAnimations: (v: boolean) => void;
  allowIcons: boolean;
  setAllowIcons: (v: boolean) => void;
  thinkingMode: ThinkingMode;
  setThinkingMode: (v: ThinkingMode) => void;
}

const resolveAllowAsciiArt = (): boolean => {
  const env = process.env.KIRO_ASCII_MODE;
  if (env === '1' || env === 'true') return false;
  return readBoolSetting(Settings.CHAT_ASCII_MODE, true);
};

/**
 * Resolve `chat.showThinking`, backwards compatible with the legacy boolean:
 * a string mode is used as-is; `false` → `off`; legacy `true` → `collapsed`;
 * missing → `expanded` (the default for new users).
 */
const resolveThinkingMode = (): ThinkingMode => {
  const val = readCliSettings()[Settings.CHAT_SHOW_THINKING];
  if (val === 'collapsed' || val === 'expanded' || val === 'off') return val;
  if (val === false) return 'off';
  if (val === true) return 'collapsed';
  return 'expanded';
};

const initialAllowAsciiArt = resolveAllowAsciiArt();
const initialAllowAnimations = readBoolSetting(Settings.CHAT_ANIMATIONS, true);
const initialAllowIcons = readBoolSetting(Settings.CHAT_ICONS, true);
const initialThinkingMode = resolveThinkingMode();

const defaultValue: GlyphsContextValue = {
  glyphs: initialAllowAsciiArt ? UNICODE_GLYPHS : ASCII_GLYPHS,
  spinners: initialAllowAsciiArt ? UNICODE_SPINNERS : ASCII_SPINNERS,
  allowAsciiArt: initialAllowAsciiArt,
  setAllowAsciiArt: () => {},
  allowAnimations: initialAllowAnimations,
  setAllowAnimations: () => {},
  allowIcons: initialAllowIcons,
  setAllowIcons: () => {},
  thinkingMode: initialThinkingMode,
  setThinkingMode: () => {},
};

export const GlyphsContext = createContext<GlyphsContextValue>(defaultValue);

export const GlyphsProvider = ({ children }: { children: React.ReactNode }) => {
  const [allowAsciiArt, setAllowAsciiArt] = useState(initialAllowAsciiArt);
  const [allowAnimations, setAllowAnimations] = useState(
    initialAllowAnimations
  );
  const [allowIcons, setAllowIcons] = useState(initialAllowIcons);
  const [thinkingMode, setThinkingMode] = useState(initialThinkingMode);

  const value = useMemo<GlyphsContextValue>(
    () => ({
      glyphs: allowAsciiArt ? UNICODE_GLYPHS : ASCII_GLYPHS,
      spinners: allowAsciiArt ? UNICODE_SPINNERS : ASCII_SPINNERS,
      allowAsciiArt,
      setAllowAsciiArt,
      allowAnimations,
      setAllowAnimations,
      allowIcons,
      setAllowIcons,
      thinkingMode,
      setThinkingMode,
    }),
    [allowAsciiArt, allowAnimations, allowIcons, thinkingMode]
  );

  return React.createElement(GlyphsContext.Provider, { value }, children);
};

export const useGlyphs = (): Glyphs => useContext(GlyphsContext).glyphs;
export const useSpinners = (): Spinners => useContext(GlyphsContext).spinners;

/**
 * Non-hook glyph resolver for modules that run outside React (slash-command
 * handlers, theme previews, feed formatting). Reads the persisted
 * `chat.allowAsciiArt` setting live, so it reflects the user's current choice
 * at call time. React components must use the `useGlyphs` hook instead so they
 * re-render when the setting is toggled in-session.
 */
export const getActiveGlyphs = (): Glyphs =>
  resolveAllowAsciiArt() ? UNICODE_GLYPHS : ASCII_GLYPHS;
export const useAllowAsciiArt = () => {
  const ctx = useContext(GlyphsContext);
  return {
    allowAsciiArt: ctx.allowAsciiArt,
    setAllowAsciiArt: ctx.setAllowAsciiArt,
  };
};
export const useAllowAnimations = () => {
  const ctx = useContext(GlyphsContext);
  return {
    allowAnimations: ctx.allowAnimations,
    setAllowAnimations: ctx.setAllowAnimations,
  };
};
export const useAllowIcons = () => {
  const ctx = useContext(GlyphsContext);
  return { allowIcons: ctx.allowIcons, setAllowIcons: ctx.setAllowIcons };
};
export const useThinkingMode = () => {
  const ctx = useContext(GlyphsContext);
  return {
    thinkingMode: ctx.thinkingMode,
    setThinkingMode: ctx.setThinkingMode,
  };
};
