import fs from 'fs';
import os from 'os';

const CONFIG_PATH = `${os.homedir()}/.fig/user/config`;

const get = () => {
  const rawConfig = String(fs.readFileSync(CONFIG_PATH));

  const vars = rawConfig.split('\n').reduce(
    (dict, line) => {
      const split = line.indexOf('=');

      const key = line.substring(0, split);
      const value = line.substring(split + 1);
      // eslint-disable-next-line no-param-reassign
      dict[key] = value;
      return dict;
    },
    {} as Record<string, unknown>
  );
  return vars;
};

const getValue = (key: string) => get()[key];

const set = (params: Record<string, unknown>, overwrite = false) => {
  const newConfig = overwrite ? params : { ...get(), ...params };
  const configString = Object.keys(newConfig)
    .reduce((out, key) => `${out}\n${key}=${newConfig[key]}`, '')
    .slice(1);

  fs.writeFileSync(CONFIG_PATH, configString);
};

const reset = () => {
  set({ FIG_ONBOARDING: 1, FIG_LOGGED_IN: 1 }, true);
};

export default { get, getValue, set, reset };
