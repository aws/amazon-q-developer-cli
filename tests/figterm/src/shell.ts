/* eslint-disable max-classes-per-file */
import * as uuid from 'uuid';
import * as pty from 'node-pty';
import os from 'os';

import { socketListen, removeListener } from './unix-server';
import { LocalMessage, ShellContext } from './local.pb';

export type PTYOptions = {
  shell: string;
  args?: string | string[];
  env?: { [key: string]: string };
  mockedCLICommands?: Record<string, string>;
};

type Watcher = {
  pattern: RegExp;
  callback: (output: string) => void;
  clearOnMatch: boolean;
};

function randomId(len: number) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ';
  let id = '';
  for (let i = 0; i < len; i += 1) {
    const idx = Math.floor(Math.random() * alphabet.length);
    id += alphabet[idx];
  }
  return id;
}

const CRLF = '\r\n';

class FigtermListener {
  id = '';

  path: string;

  sessionId: string;

  callback: (msg: LocalMessage) => void = () => {};

  nextPrompt: Promise<void>;

  onPrompt = () => {};

  buffer = '';

  cursor = 0;

  constructor(path: string, sessionId: string) {
    this.path = path;
    this.sessionId = sessionId;

    const makeNextPromptPromise = () =>
      new Promise<void>((resolve) => {
        this.onPrompt = () => {
          resolve();
          this.nextPrompt = makeNextPromptPromise();
        };
      });

    this.nextPrompt = makeNextPromptPromise();
    this.listen();
  }

  onMessage(message: LocalMessage) {
    const matchesSession = (context: ShellContext | undefined) =>
      context?.sessionId === this.sessionId;

    switch (message.type?.$case) {
      case 'hook': {
        const { hook } = message.type.hook;
        switch (hook?.$case) {
          case 'init': {
            break;
          }
          case 'prompt': {
            if (matchesSession(hook.prompt.context)) {
              this.onPrompt();
            }
            break;
          }
          case 'editBuffer': {
            if (matchesSession(hook.editBuffer.context)) {
              this.buffer = hook.editBuffer.text;
              this.cursor = hook.editBuffer.cursor;
            }
            break;
          }
          default: {
            break;
          }
        }
        break;
      }
      case 'command': {
        break;
      }
      default: {
        break;
      }
    }
  }

  listen() {
    this.id = socketListen(this.path, (data) => {
      let buf = data;
      while (buf.length > 0) {
        const dataType = buf.slice(2, 10).toString();
        const lenBytes = buf.slice(10, 18);
        let len = 0;
        for (let i = 0; i < lenBytes.length; i += 1) {
          len = len * 256 + lenBytes[i];
        }

        const msg = buf.slice(18, 18 + len);
        if (dataType === 'fig-json') {
          this.onMessage(LocalMessage.fromJSON(JSON.parse(msg.toString())));
        } else {
          this.onMessage(LocalMessage.decode(msg));
        }
        buf = buf.slice(18 + len);
      }
    });
  }

  async restart() {
    await this.stop();
    this.listen();
  }

  async stop() {
    await removeListener(this.path, this.id);
  }
}

class FigCliListener {
  id: string;

  commands: string[] = [];

  constructor(sessionId: string, path = '/tmp/mock_q_cli.socket') {
    this.id = socketListen(path, (data) => {
      const message = String(Buffer.from(data.toString(), 'base64'));
      const tokens = message.slice(0, -1).split(' ');
      if (tokens[2] === sessionId) {
        const command = `fig ${tokens.slice(3).join(' ')}`.trim();
        if (tokens[1] !== '0') {
          console.error(`Error running fig command "${command}"`);
        } else {
          this.commands.push(command);
        }
      }
    });
  }

  async stop() {
    await removeListener('/tmp/mock_q_cli.socket', this.id);
  }

  reset() {
    this.commands = [];
  }
}

class Shell {
  cliListener: FigCliListener | undefined;

  figtermListener: FigtermListener | undefined;

  pty: pty.IPty | undefined;

  exitPty: (signal?: string) => Promise<void> = async () => {};

  initialEnv: Record<string, string> = {};

  startupTime = -1;

  commandOutputWatchers: Watcher[] = [];

  sessionBuffer = '';

  get buffer() {
    return this.figtermListener?.buffer ?? '';
  }

  get cursor() {
    return this.figtermListener?.cursor ?? 0;
  }

  get figCommands() {
    return this.cliListener?.commands ?? [];
  }

  async initialize({ shell, args, env, mockedCLICommands }: PTYOptions) {
    if (this.cliListener || this.figtermListener || this.pty) {
      await this.kill();
    }

    this.sessionBuffer = '';
    this.commandOutputWatchers = [];
    this.startupTime = -1;

    const environment = Object.entries(env || process.env).reduce(
      (acc, [key, val]) => {
        if (!key.startsWith('FIG') && val !== undefined) {
          acc[key] = val;
        }
        return acc;
      },
      {} as Record<string, string>
    );

    if (mockedCLICommands) {
      environment.PATH = `${__dirname}/bin:${environment.PATH}`;

      Object.keys(mockedCLICommands).forEach((key) => {
        environment[`MOCK_${key.replaceAll(':', '_')}`] =
          mockedCLICommands[key];
      });
    }

    this.initialEnv = {
      ...environment,
      TMPDIR: '/tmp/',
      TERM_SESSION_ID: uuid.v4(),
      FIG_SHELL_EXTRA_ARGS: Array.isArray(args) ? args.join(' ') : (args ?? ''),
    };

    this.cliListener = new FigCliListener(this.initialEnv.TERM_SESSION_ID);
    this.figtermListener = new FigtermListener(
      // TODO: fix this is not the correct path anymore
      `/var/tmp/fig/${os.userInfo().username}/desktop.socket`,
      this.initialEnv.TERM_SESSION_ID
    );
    const firstPrompt = this.figtermListener.nextPrompt;

    const start = Date.now();
    firstPrompt.then(() => {
      this.startupTime = Date.now() - start;
    });

    this.pty = pty.spawn(shell, args ?? [], {
      name: 'xterm-color',
      cols: 80,
      rows: 30,
      cwd: process.env.HOME,
      env: this.initialEnv,
    });

    let commandOutputBuffer = '';
    this.pty.onData((data) => {
      commandOutputBuffer += data;
      this.sessionBuffer += data;
      let shouldClear = false;
      this.commandOutputWatchers.filter((watcher) => {
        const { pattern, callback, clearOnMatch } = watcher;
        const matches = commandOutputBuffer.match(pattern);
        if (matches) {
          const output = matches[1] ?? '';
          callback(output);
          if (clearOnMatch) {
            shouldClear = true;
          }
          return false;
        }
        return true;
      });
      if (shouldClear) {
        commandOutputBuffer = '';
      }
    });

    this.exitPty = (signal?: string) => {
      const prom = new Promise<void>((resolve) =>
        this.pty?.onExit(() => resolve())
      );
      this.pty?.kill(signal ?? 'SIGKILL');
      this.pty = undefined;

      return prom;
    };

    return firstPrompt;
  }

  async restartFigtermListener() {
    if (!this.figtermListener) throw new Error('Initialize shell first');
    await this.figtermListener.restart();
  }

  waitForNextPrompt() {
    if (!this.figtermListener) throw new Error('Initialize shell first');
    return this.figtermListener.nextPrompt;
  }

  mockFigCommand({ command, value }: { command: string; value: string }) {
    return this.execute(`export MOCK_${command.replaceAll(':', '_')}=${value}`);
  }

  write(text: string) {
    if (!this.pty) throw new Error('Initialize shell first');
    this.pty.write(text);
  }

  resize({ rows, cols }: { rows: number; cols: number }) {
    if (!this.pty) throw new Error('Initialize shell first');
    this.pty.resize(cols, rows);
  }

  execute(command: string, promptTimeout = 100) {
    return new Promise<string>((resolve, reject) => {
      const nextPrompt = this.waitForNextPrompt();
      const callback = (output: string) => {
        // Wait for next prompt before resolving.
        Promise.race([
          nextPrompt,
          new Promise<void>((_, r2) => setTimeout(r2, promptTimeout)),
        ])
          .then(() => resolve(output))
          .catch(() => {
            reject(
              new Error(
                `Timed out waiting ${promptTimeout}ms for prompt ` +
                  `after command '${command}' output '${output}'`
              )
            );
          });
      };

      const wrapper = `-----${randomId(5)}-----`;
      const [prefix, suffix] = [`<<<${wrapper}`, `${wrapper}>>>`];
      this.commandOutputWatchers.push({
        pattern: new RegExp(`${prefix}${CRLF}(.*?)${CRLF}${suffix}`, 'ms'),
        clearOnMatch: true,
        callback,
      });
      this.write(`echo "${prefix}" ; ${command} ; echo "${suffix}"\r`);
    });
  }

  type(text: string) {
    return new Promise<void>((resolve) => {
      const chars = text.split('');
      const interval = setInterval(() => {
        if (chars.length === 0) {
          setTimeout(resolve, 500);
          clearInterval(interval);
          return;
        }
        const c = chars.shift();
        this.write(c ?? '');
      }, 10);
    });
  }

  getEnv() {
    return this.execute('env').then((env) =>
      env.split(CRLF).reduce(
        (dict, line) => {
          const [key, ...valueParts] = line.split('=');
          // eslint-disable-next-line no-param-reassign
          dict[key] = valueParts.join('=');
          return dict;
        },
        {} as Record<string, string>
      )
    );
  }

  getSessionTranscript() {
    return this.sessionBuffer;
  }

  async kill(signal?: string) {
    await this.cliListener?.stop();
    await this.figtermListener?.stop();
    await this.exitPty(signal);
    this.cliListener = undefined;
    this.figtermListener = undefined;
    this.exitPty = async () => {};
  }
}

export default Shell;
