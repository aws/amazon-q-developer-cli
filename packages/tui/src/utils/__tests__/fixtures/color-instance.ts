import { chalk } from '../../color.js';

console.log(
  JSON.stringify({
    level: chalk.level,
    forceColor: process.env.FORCE_COLOR ?? null,
  })
);
