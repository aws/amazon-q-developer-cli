export function sanitizeSessionTitleForDisplay(
  title: string | null | undefined
): string {
  if (title == null || title === '') return '';
  return title.replace(/\r\n?|\n/g, '\\n');
}
