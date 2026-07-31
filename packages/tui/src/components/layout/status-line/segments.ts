import type React from 'react';
import type { Glyphs, Spinners } from '../../../utils/glyphs.js';

/**
 * What the theme's colour lookup hands back: a chalk function that also carries
 * the raw value, for the places that need a hex rather than a wrapper.
 */
export type SegmentColor = ((text: string) => string) & { hex?: string };
import type { StatusSurfaceProps } from '../status-surface.js';

/**
 * Paint order, fixed: configuration decides visibility only.
 *
 * One order serves both surfaces. The full TUI partitions it by `side`, so
 * `location`/`branch` sitting mid-list still land after everything on the left.
 * Lite walks it as-is, which is why they precede `goal`: lite trails goal.
 */
export const STATUS_SEGMENT_IDS = [
  'agent',
  'autonomous',
  'model',
  'effort',
  'context',
  'tangent',
  'codeIntel',
  'location',
  'branch',
  'goal',
  'date',
  'time',
  'usage',
  'credits',
] as const;

export type StatusSegmentId = (typeof STATUS_SEGMENT_IDS)[number];

const ID_SET: ReadonlySet<string> = new Set(STATUS_SEGMENT_IDS);

export function isStatusSegmentId(value: unknown): value is StatusSegmentId {
  return typeof value === 'string' && ID_SET.has(value);
}

export interface SegmentRenderContext {
  props: StatusSurfaceProps;
  getColor: (colorPath: string) => SegmentColor;
  glyphs: Glyphs;
  allowIcons: boolean;
  spinners: Spinners;
  /**
   * Non-null only while the modern bar renders dimmed, in which case every
   * segment collapses to this colour instead of its own.
   */
  muted: ((text: string) => string) | null;
  /** Wall clock for the `date`/`time` segments; absent means neither is shown. */
  now: Date | null;
}

/**
 * Declaring a renderer per surface is what stops a segment from rendering on one
 * and silently vanishing on the other; the two are genuinely forked, React chips
 * versus ANSI strings.
 */
export interface StatusSegmentDef {
  side: 'left' | 'right';
  tui(ctx: SegmentRenderContext): React.ReactNode | null;
  /** Return '' to omit. Null means lite has no equivalent and never offers it. */
  lite: ((ctx: SegmentRenderContext) => string) | null;
}

/** Reading a clock, so the surface has to tick while one of these is shown. */
export const CLOCK_SEGMENT_IDS: readonly StatusSegmentId[] = ['date', 'time'];

/** Backed by a fetch, so nothing is requested unless one of these is shown. */
export const BILLING_SEGMENT_IDS: readonly StatusSegmentId[] = [
  'usage',
  'credits',
];
