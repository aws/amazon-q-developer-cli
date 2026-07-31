/**
 * Subscribed rather than copied into state: the configuration lives in a file, so
 * a write elsewhere has to repaint the bar without the caller holding a stale map.
 */
import { useSyncExternalStore } from 'react';
import type { UiMode } from '../../../types/ui-mode.js';
import {
  getStatusSegments,
  subscribeStatusLine,
  type StatusSegmentVisibility,
} from './config.js';

export function useStatusSegments(surface: UiMode): StatusSegmentVisibility {
  const read = () => getStatusSegments(surface);
  return useSyncExternalStore(subscribeStatusLine, read, read);
}
