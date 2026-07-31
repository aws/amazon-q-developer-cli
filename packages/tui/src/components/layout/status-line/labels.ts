/**
 * User-facing names for the status-line segments.
 *
 * Kept apart from the renderers so the menu can list and describe segments
 * without importing React chips, and so wording changes never risk touching
 * paint code.
 *
 * Groups name a segment's default rather than its current visibility: the menu
 * already marks every row on or off, and a group claiming a segment is shown would
 * contradict that marker the moment someone turns it off. Anything conditional
 * about when a segment has a value belongs in its description.
 *
 * Data only, so listing and describing segments does not pull in the renderers.
 *
 * The record is total over `StatusSegmentId`, so a new segment cannot ship
 * without a name for the menu to show.
 */
import type { StatusSegmentId } from './segments.js';

export const STATUS_SEGMENT_GROUPS = [
  'On by default',
  'Off by default',
] as const;

export type StatusSegmentGroup = (typeof STATUS_SEGMENT_GROUPS)[number];

export interface StatusSegmentLabel {
  label: string;
  description: string;
}

export const STATUS_SEGMENT_LABELS: Record<
  StatusSegmentId,
  StatusSegmentLabel
> = {
  agent: {
    label: 'Agent',
    description: 'Active agent name',
  },
  autonomous: {
    label: 'Autonomous',
    description: 'Only when running unattended',
  },
  model: {
    label: 'Model',
    description: 'Active model name',
  },
  effort: {
    label: 'Effort',
    description: 'Reasoning effort, if reported',
  },
  context: {
    label: 'Context',
    description: 'Share of the context used',
  },
  location: {
    label: 'Location',
    description: 'Working directory or repo',
  },
  branch: {
    label: 'Git branch',
    description: 'Checked-out branch',
  },
  tangent: {
    label: 'Tangent',
    description: 'Only on a tangent',
  },
  codeIntel: {
    label: 'Code intelligence',
    description: 'Only while code intel runs',
  },
  goal: {
    label: 'Goal',
    description: 'Only in goal mode: progress',
  },
  date: {
    label: 'Date',
    description: 'Current date',
  },
  time: {
    label: 'Time',
    description: 'Current time',
  },
  usage: {
    label: 'Usage',
    description: 'Allowance used, limited plans',
  },
  credits: {
    label: 'Credits',
    description: 'Credits left, limited plans',
  },
};
