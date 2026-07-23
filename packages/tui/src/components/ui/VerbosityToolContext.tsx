import React from 'react';
import type { ToolArgsMode } from '../../lite/verbose.js';
import { useExpandableRegistration } from '../../hooks/useExpandableOutput.js';

type VerbosityToolContextValue = {
  outputVisible: boolean;
  reasoning?: string;
  elapsedMs?: number;
  argsMode?: ToolArgsMode;
  argsMaxLines?: number | null;
  argsMaxChars?: number | null;
  argsExpanded?: boolean;
  isStatic?: boolean;
};

export const VerbosityToolContext =
  React.createContext<VerbosityToolContextValue>({ outputVisible: true });

export const useVerbosityToolContext = (): VerbosityToolContextValue =>
  React.useContext(VerbosityToolContext);

export const useToolOutputVisible = (): boolean =>
  useVerbosityToolContext().outputVisible;

export const useToolArgsExpanded = (truncated: boolean): boolean => {
  const { argsMode, argsExpanded, isStatic } = useVerbosityToolContext();
  useExpandableRegistration(truncated && argsMode !== 'off' && !isStatic);
  return argsMode !== 'off' && argsExpanded === true;
};
