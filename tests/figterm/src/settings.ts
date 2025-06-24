import fs from 'fs';
import os from 'os';

const SETTINGS_PATH = `${os.homedir()}/.fig/settings.json`;

const get = () => JSON.parse(String(fs.readFileSync(SETTINGS_PATH)));
const getValue = (key: string) => get()[key];

const set = (params: Record<string, unknown>, overwrite = false) => {
  const newSettings = overwrite ? params : { ...get(), ...params };
  fs.writeFileSync(SETTINGS_PATH, JSON.stringify(newSettings, null, 4));
};

const reset = () => set({}, true);

export default { get, getValue, set, reset };
