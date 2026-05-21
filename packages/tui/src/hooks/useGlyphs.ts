import React, { createContext, useContext, useState, useMemo } from 'react';
import {
  UNICODE_GLYPHS,
  ASCII_GLYPHS,
  UNICODE_SPINNERS,
  ASCII_SPINNERS,
  type Glyphs,
  type Spinners,
} from '../utils/glyphs.js';
import { readBoolSetting } from '../utils/cli-settings.js';
import { Settings } from '../constants/settings.js';

interface GlyphsContextValue {
  glyphs: Glyphs;
  spinners: Spinners;
  allowAsciiArt: boolean;
  setAllowAsciiArt: (v: boolean) => void;
  allowAnimations: boolean;
  setAllowAnimations: (v: boolean) => void;
  allowIcons: boolean;
  setAllowIcons: (v: boolean) => void;
  showThinking: boolean;
  setShowThinking: (v: boolean) => void;
}

const resolveAllowAsciiArt = (): boolean => {
  const env = process.env.KIRO_ASCII_MODE;
  if (env === '1' || env === 'true') return false;
  return readBoolSetting(Settings.CHAT_ASCII_MODE, true);
};

const initialAllowAsciiArt = resolveAllowAsciiArt();
const initialAllowAnimations = readBoolSetting(Settings.CHAT_ANIMATIONS, true);
const initialAllowIcons = readBoolSetting(Settings.CHAT_ICONS, true);
const initialShowThinking = readBoolSetting(Settings.CHAT_SHOW_THINKING, true);

const defaultValue: GlyphsContextValue = {
  glyphs: initialAllowAsciiArt ? UNICODE_GLYPHS : ASCII_GLYPHS,
  spinners: initialAllowAsciiArt ? UNICODE_SPINNERS : ASCII_SPINNERS,
  allowAsciiArt: initialAllowAsciiArt,
  setAllowAsciiArt: () => {},
  allowAnimations: initialAllowAnimations,
  setAllowAnimations: () => {},
  allowIcons: initialAllowIcons,
  setAllowIcons: () => {},
  showThinking: initialShowThinking,
  setShowThinking: () => {},
};

export const GlyphsContext = createContext<GlyphsContextValue>(defaultValue);

export const GlyphsProvider = ({ children }: { children: React.ReactNode }) => {
  const [allowAsciiArt, setAllowAsciiArt] = useState(initialAllowAsciiArt);
  const [allowAnimations, setAllowAnimations] = useState(
    initialAllowAnimations
  );
  const [allowIcons, setAllowIcons] = useState(initialAllowIcons);
  const [showThinking, setShowThinking] = useState(initialShowThinking);

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
      showThinking,
      setShowThinking,
    }),
    [allowAsciiArt, allowAnimations, allowIcons, showThinking]
  );

  return React.createElement(GlyphsContext.Provider, { value }, children);
};

export const useGlyphs = (): Glyphs => useContext(GlyphsContext).glyphs;
export const useSpinners = (): Spinners => useContext(GlyphsContext).spinners;
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
export const useShowThinking = () => {
  const ctx = useContext(GlyphsContext);
  return {
    showThinking: ctx.showThinking,
    setShowThinking: ctx.setShowThinking,
  };
};
