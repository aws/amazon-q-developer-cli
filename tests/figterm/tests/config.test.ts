import Shell from '../src/shell';
import Config from '../src/config';
import Settings from '../src/settings';

const UPDATE_ALERT = new RegExp('Updating .*? to latest version...');
const VERSION_AVAILABLE = new RegExp('A new version of .*? is available.');

const AUTOUPDATE_TEXT =
  '(To turn off automatic updates, run `fig settings app.disableAutoupdates true`)';

const shell = new Shell();
let mockedCLICommands: Record<string, string> = {};

beforeEach(async () => {
  Config.reset();
  Settings.reset();
  Config.set({ NEW_VERSION_AVAILABLE: 'v1.0.49' });
  mockedCLICommands = { 'app:running': '1' };
});

afterEach(async () => {
  await shell.kill();
});

describe('Testing ~/.fig/user/config', () => {
  test('New version available (show hint)', async () => {
    await shell.initialize({ shell: 'bash', mockedCLICommands });
    const transcript = shell.getSessionTranscript();

    expect(transcript).toMatch(UPDATE_ALERT);
    expect(transcript).toContain(AUTOUPDATE_TEXT);
    expect(shell.figCommands).toContain('fig app:running');
    expect(shell.figCommands).toContain('fig update:app --force');
    expect(Config.getValue('DISPLAYED_AUTOUPDATE_SETTINGS_HINT')).toBe('1');
  });

  test('New version available (do not show hint)', async () => {
    Config.set({ DISPLAYED_AUTOUPDATE_SETTINGS_HINT: '1' });
    await shell.initialize({ shell: 'bash', mockedCLICommands });
    const transcript = shell.getSessionTranscript();

    expect(transcript).toMatch(UPDATE_ALERT);
    expect(transcript).not.toContain(AUTOUPDATE_TEXT);
    expect(shell.figCommands).toContain('fig app:running');
    expect(shell.figCommands).toContain('fig update:app --force');
  });

  test('New version available (app not running)', async () => {
    mockedCLICommands['app:running'] = '';
    await shell.initialize({ shell: 'bash', mockedCLICommands });
    const transcript = shell.getSessionTranscript();

    expect(transcript).not.toMatch(UPDATE_ALERT);
    expect(transcript).not.toContain(AUTOUPDATE_TEXT);
    expect(shell.figCommands).toContain('fig app:running');
    expect(shell.figCommands).not.toContain('fig update:app --force');
  });

  test('New version available. Autoupdates disabled.', async () => {
    Settings.set({ 'app.disableAutoupdates': true });
    await shell.initialize({ shell: 'bash', mockedCLICommands });
    const transcript = shell.getSessionTranscript();

    expect(transcript).toMatch(VERSION_AVAILABLE);
    expect(transcript).not.toContain(AUTOUPDATE_TEXT);
    expect(shell.figCommands).toContain('fig app:running');
    expect(shell.figCommands).toContain('fig settings app.disableAutoupdates');
    expect(shell.figCommands).not.toContain('fig update:app --force');
  });
});
