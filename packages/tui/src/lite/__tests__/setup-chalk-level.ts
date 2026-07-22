// The render modules build `chalk.hex(...)` stylers at module load, so the
// level must be set before they load. Importing this file first pins the
// shared instance to truecolor so those stylers emit the SGR the assertions
// expect.
import { chalk } from '../../utils/color.js';

chalk.level = 3;
