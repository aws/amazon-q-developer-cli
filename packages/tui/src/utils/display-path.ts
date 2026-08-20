export function displayBasename(value: string): string {
  const separator = Math.max(value.lastIndexOf('/'), value.lastIndexOf('\\'));
  return separator >= 0 && separator < value.length - 1
    ? value.slice(separator + 1)
    : value;
}
