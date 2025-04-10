import Shell, { PTYOptions } from '../src/shell';

const filterOutliers = (arr: number[]): number[] => {
  const values = [...arr];
  values.sort((a, b) => a - b);

  const q1 = values[Math.floor(values.length / 4)];
  const q3 = values[Math.ceil(values.length * (3 / 4))];
  const iqr = q3 - q1;

  const maxValue = q3 + iqr * 1.5;
  const minValue = q1 - iqr * 1.5;

  return values.filter((x) => x <= maxValue && x >= minValue);
};

const computeAverageStartupTime = async (opts: PTYOptions, n = 5) => {
  const times: number[] = [];
  for (let i = 0; i < n; i += 1) {
    const shell = new Shell();
    // eslint-disable-next-line no-await-in-loop
    await shell.initialize(opts);
    times.push(shell.startupTime);
    // eslint-disable-next-line no-await-in-loop
    await shell.kill();
  }
  const filtered = filterOutliers(times);
  return filtered.reduce((a, b) => a + b) / filtered.length;
};

test('zsh: fig startup time', async () => {
  const shell = 'zsh';
  const getEnv = (fig: boolean) => ({
    ...process.env,
    ZDOTDIR: `/usr/home/with${fig ? '' : 'out'}fig`,
  });
  const figMinimal = await computeAverageStartupTime(
    { shell, env: getEnv(true) },
    100
  );
  const withoutFig = await computeAverageStartupTime(
    { shell, env: getEnv(false) },
    100
  );
  expect(figMinimal).toBeLessThan(withoutFig + 50);
}, 20000);

test('bash: fig startup time', async () => {
  const shell = 'bash';
  const getArgs = (fig: boolean) => [
    '--init-file',
    `/usr/home/with${fig ? '' : 'out'}fig/.bashrc`,
  ];
  const figMinimal = await computeAverageStartupTime(
    { shell, args: getArgs(true) },
    100
  );
  const withoutFig = await computeAverageStartupTime(
    { shell, args: getArgs(false) },
    100
  );
  expect(figMinimal).toBeLessThan(withoutFig + 50);
}, 20000);
