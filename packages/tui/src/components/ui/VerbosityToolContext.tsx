import React from 'react';
import {
  TUI_DEFAULT_DISPLAY,
  type ToolArgsMode,
  type VerboseDisplayConfig,
} from '../../lite/verbose.js';
import { useExpandableRegistration } from '../../hooks/useExpandableOutput.js';

export type ToolDisplayPolicy = {
  display: VerboseDisplayConfig;
  outputVisible: boolean;
  reasoning?: string;
  elapsedMs?: number;
  argsMode?: ToolArgsMode;
  argsMaxLines?: number | null;
  argsMaxChars?: number | null;
  argsExpanded?: boolean;
  isStatic?: boolean;
};

export const VerbosityToolContext = React.createContext<ToolDisplayPolicy>({
  display: TUI_DEFAULT_DISPLAY,
  outputVisible: true,
});

export const useVerbosityToolContext = (): ToolDisplayPolicy =>
  React.useContext(VerbosityToolContext);

export const useToolDisplayPolicy = (): ToolDisplayPolicy =>
  useVerbosityToolContext();

export const useToolOutputVisible = (): boolean =>
  useVerbosityToolContext().outputVisible;

export const useToolArgsExpanded = (truncated: boolean): boolean => {
  const { argsMode, argsExpanded, isStatic } = useVerbosityToolContext();
  useExpandableRegistration(truncated && argsMode !== 'off' && !isStatic);
  return argsMode !== 'off' && argsExpanded === true;
};
