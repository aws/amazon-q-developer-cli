/**
 * Force chalk to truecolor (level 3) before the lite render/diff tests
 * import any production code.
 *
 * Why this lives in setupFiles (not at the top of each test file):
 *   - `src/lite/render.ts` and `src/lite/diff.ts` build chalk wrappers at
 *     module load time:
 *       const brand = chalk.hex('#C19AFF');
 *       const greenTint = chalk.hex('#a3c0a3');
 *     These wrappers capture chalk's level at construction time, so the
 *     SGR they emit is decided BEFORE the test body runs.
 *   - ES module imports are hoisted. A test file that imports the
 *     production module on line 1 and does `chalk.level = 3` on line 50
 *     bumps the level too late: the brand/green wrappers were already
 *     built at chalk's auto-detected boot level (0–1 under bun-spawned
 *     vitest in non-TTY).
 *   - vitest's `setupFiles` runs in the same worker, BEFORE the test
 *     module graph is loaded. Mutating chalk here is the earliest hook
 *     we have, so the wrappers built downstream pick up level 3.
 *
 * What the four affected tests assert (and why they need level 3):
 *   - diff.test.ts > syntax highlighting carries through wrap boundaries
 *       (cli-highlight emits SGR only when chalk.level >= 1; we need
 *        level 3 because the assertion looks for the comment-token color)
 *   - render.test.ts > code block with trailing ANSI reset does not bleed
 *       (asserts `\x1b[31m` from cli-highlight bash highlighting)
 *   - render.test.ts > renderShellOutputBlock uses brand color for gutter
 *       (asserts truecolor `\x1b[38;2;193;154;255m` from chalk.hex)
 *   - render.test.ts > json-shape tool output bodies pick up green tint
 *       (asserts truecolor `\x1b[38;2;163;192;163m` from chalk.hex)
 */
import chalk from 'chalk';

process.env.FORCE_COLOR = '3';
chalk.level = 3;
