import { UNICODE_GLYPHS, type Glyphs } from './glyphs';

/**
 * Persistent `Cloud · <repo>` footer label for a cloud session (not the activity
 * status). `repo` is bound at session start; omitted for a New empty sandbox,
 * which shows just `Cloud`.
 */
export function formatCloudFooter(
  repo?: string | null,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  const label = 'Cloud';
  const trimmed = repo?.trim();
  return trimmed ? `${label} ${glyphs.smallDot} ${trimmed}` : label;
}
