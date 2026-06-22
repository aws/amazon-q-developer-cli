// chalk wrappers (chalk.hex(...) in render.ts/diff.ts) capture chalk.level at
// module-load time, and ESM imports hoist — so a `chalk.level = 3` in a test
// body runs too late. Set it here: setupFiles runs before the test module graph
// loads, so the wrappers built downstream emit truecolor SGR the assertions need.
import chalk from 'chalk';

process.env.FORCE_COLOR = '3';
chalk.level = 3;
