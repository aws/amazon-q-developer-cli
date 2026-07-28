import type { WorkflowMonitorLayout } from '../../../types/workflow-monitor.js';

export interface MonitorPaneDimensions {
  dagWidth: number;
  dagHeight: number;
  outputWidth: number;
  outputHeight: number;
}

export const MONITOR_RESIZE_STEP = 0.05;

export function monitorSplitDirection(
  layout: WorkflowMonitorLayout
): 'row' | 'column' {
  return layout === 'side-by-side' ? 'row' : 'column';
}

export function monitorPaneDimensions(
  layout: WorkflowMonitorLayout,
  width: number,
  height: number,
  ratio: number
): MonitorPaneDimensions {
  if (layout === 'side-by-side') {
    const usable = Math.max(0, width - 1);
    const dagWidth = Math.max(1, Math.round(usable * ratio));
    return {
      dagWidth,
      dagHeight: height,
      outputWidth: Math.max(1, usable - dagWidth),
      outputHeight: height,
    };
  }

  const usable = Math.max(0, height - 1);
  const dagHeight = Math.max(1, Math.round(usable * ratio));
  return {
    dagWidth: width,
    dagHeight,
    outputWidth: width,
    outputHeight: Math.max(1, usable - dagHeight),
  };
}

export function resizeMonitorRatio(ratio: number, delta: number): number {
  return Math.min(0.8, Math.max(0.2, ratio + delta));
}
