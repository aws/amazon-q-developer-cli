import type {
  TerminalCellStyle,
  TerminalColor,
  TerminalFrame,
} from '../test-utils/shared/terminal-frame.js';
import type {
  StorybookAssertions,
  StorybookTerminalColor,
  StorybookTextStyleAssertion,
} from './contracts.js';

function terminalColorLabel(color: TerminalColor): StorybookTerminalColor {
  if (color.mode === 'default') return 'default';
  if (color.mode === 'palette') return `palette:${color.value}`;
  return `#${color.value.toString(16).padStart(6, '0')}`;
}

function matchingStyles(
  frame: TerminalFrame,
  assertion: StorybookTextStyleAssertion
): readonly TerminalCellStyle[] | null {
  if (assertion.text.length === 0 || (assertion.occurrence ?? 1) < 1) {
    return null;
  }
  let remaining = assertion.occurrence ?? 1;
  for (const row of frame.rows) {
    const cells = row.filter((cell) => cell.width !== 0);
    const text = cells.map((cell) => cell.text).join('');
    let offset = 0;
    while (offset <= text.length - assertion.text.length) {
      const matchStart = text.indexOf(assertion.text, offset);
      if (matchStart === -1) break;
      remaining -= 1;
      if (remaining === 0) {
        const matchEnd = matchStart + assertion.text.length;
        let cellStart = 0;
        return cells.flatMap((cell) => {
          const cellEnd = cellStart + cell.text.length;
          const overlaps = cellStart < matchEnd && cellEnd > matchStart;
          cellStart = cellEnd;
          return overlaps ? [frame.styles[cell.styleIndex]!] : [];
        });
      }
      offset = matchStart + assertion.text.length;
    }
  }
  return null;
}

function styleFailures(
  frame: TerminalFrame,
  assertion: StorybookTextStyleAssertion
): string[] {
  const styles = matchingStyles(frame, assertion);
  if (!styles || styles.length === 0) {
    return [
      `missing styled "${assertion.text}" occurrence ${assertion.occurrence ?? 1}`,
    ];
  }
  const failures: string[] = [];
  const colorChecks = [
    ['foreground', assertion.foreground],
    ['background', assertion.background],
  ] as const;
  for (const [property, expected] of colorChecks) {
    if (expected === undefined) continue;
    const actual = [
      ...new Set(styles.map((style) => terminalColorLabel(style[property]))),
    ];
    if (actual.length !== 1 || actual[0] !== expected.toLowerCase()) {
      failures.push(
        `styled "${assertion.text}" expected ${property} ${expected}, found ${actual.join(', ')}`
      );
    }
  }
  const booleanChecks = [
    'bold',
    'dim',
    'italic',
    'underline',
    'inverse',
  ] as const;
  for (const property of booleanChecks) {
    const expected = assertion[property];
    if (
      expected !== undefined &&
      styles.some((style) => style[property] !== expected)
    ) {
      failures.push(
        `styled "${assertion.text}" expected ${property}=${String(expected)}`
      );
    }
  }
  return failures;
}

export function storyFrameAssertionFailures(
  frame: TerminalFrame,
  assertions: StorybookAssertions | undefined
): string[] {
  const screen = frame.rows
    .map((row) =>
      row
        .filter((cell) => cell.width !== 0)
        .map((cell) => cell.text)
        .join('')
    )
    .join('\n');
  const failures: string[] = [];
  for (const expected of assertions?.visible ?? []) {
    if (!screen.includes(expected)) failures.push(`missing "${expected}"`);
  }
  for (const forbidden of assertions?.hidden ?? []) {
    if (screen.includes(forbidden)) {
      failures.push(`unexpected "${forbidden}"`);
    }
  }
  let orderedOffset = 0;
  for (const expected of assertions?.ordered ?? []) {
    const index = screen.indexOf(expected, orderedOffset);
    if (index === -1) {
      failures.push(`missing ordered "${expected}"`);
      break;
    }
    orderedOffset = index + expected.length;
  }
  for (const [expected, count] of Object.entries(
    assertions?.occurrences ?? {}
  )) {
    let actual = 0;
    let offset = 0;
    while (expected.length > 0) {
      const index = screen.indexOf(expected, offset);
      if (index === -1) break;
      actual += 1;
      offset = index + expected.length;
    }
    if (actual !== count) {
      failures.push(
        `expected "${expected}" ${count} time${count === 1 ? '' : 's'}, found ${actual}`
      );
    }
  }
  for (const assertion of assertions?.styled ?? []) {
    failures.push(...styleFailures(frame, assertion));
  }
  return failures;
}
