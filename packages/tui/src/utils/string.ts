/**
 * Normalize line endings to \n (handles \r\n and \r)
 */
export const normalizeLineEndings = (str: string): string =>
  str.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

/**
 * Check if string contains only printable characters (including newlines and tabs)
 */
export const isPrintable = (str: string): boolean =>
  Array.from(str).every((c) => {
    const code = c.codePointAt(0)!;
    // Allow: tab (9), newline (10), carriage return (13), and anything >= 32
    // except DEL (127) and C1 control characters (128-159)
    return (
      code === 9 ||
      code === 10 ||
      code === 13 ||
      (code >= 32 && code !== 127 && !(code >= 128 && code <= 159))
    );
  });

/**
 * Shorten a file path by replacing the home directory with ~
 * @param path - The full path to shorten
 * @returns The shortened path with ~ for home directory
 */
export const shortenPath = (path: string): string => {
  const home = process.env.HOME || process.env.USERPROFILE;
  if (!home) return path;

  // Replace home directory with ~
  if (path.startsWith(home)) {
    return path.replace(home, '~');
  }

  return path;
};
