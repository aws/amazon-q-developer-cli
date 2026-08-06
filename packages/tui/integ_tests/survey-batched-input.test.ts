import { describe, expect, it } from 'bun:test';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const hasTmux = spawnSync('tmux', ['-V']).status === 0;
const tmuxIt = hasTmux ? it : it.skip;

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'\\''`)}'`;
}

function tmux(socket: string, args: string[]) {
  return spawnSync('tmux', ['-S', socket, ...args], {
    encoding: 'utf8',
  });
}

async function waitFor(check: () => boolean, timeoutMs = 5000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (check()) return;
    await Bun.sleep(25);
  }
  throw new Error('Timed out waiting for tmux survey fixture');
}

describe('SurveyPanel batched input', () => {
  tmuxIt(
    'preserves a printable chunk sent through tmux',
    async () => {
      // Only survey eligibility is injected; tmux, Twinki, and input handling stay production.
      const dir = mkdtempSync(join(tmpdir(), 'kiro-survey-tmux-'));
      const fixtureDir = mkdtempSync(join(import.meta.dir, '.survey-fixture-'));
      const socket = join(dir, 'tmux.sock');
      const resultPath = join(dir, 'answer.json');
      const fixturePath = join(fixtureDir, 'survey-fixture.tsx');
      const fixtureLogPath = join(dir, 'fixture.log');
      const tuiRoot = resolve(import.meta.dir, '..');
      const imports = {
        survey: pathToFileURL(
          join(tuiRoot, 'src/components/ui/SurveyPanel.tsx')
        ).href,
        store: pathToFileURL(join(tuiRoot, 'src/stores/app-store.ts')).href,
        theme: pathToFileURL(join(tuiRoot, 'src/theme/ThemeProvider.tsx')).href,
        renderer: pathToFileURL(join(tuiRoot, 'src/renderer.ts')).href,
      };
      const feedback = 'I got a /mcp error on startup';

      writeFileSync(
        fixturePath,
        `import React from 'react';
import { writeFileSync } from 'node:fs';
import { SurveyPanel } from ${JSON.stringify(imports.survey)};
import { AppStoreContext, createAppStore } from ${JSON.stringify(imports.store)};
import { ThemeProvider } from ${JSON.stringify(imports.theme)};
import { render } from ${JSON.stringify(imports.renderer)};

const store = createAppStore({ kiro: {}, agentEngine: 'kas' });
store.setState({
  activeSurvey: {
    id: 'tmux-feedback',
    title: 'Feedback',
    questions: [{
      id: 'feedback',
      prompt: 'Type feedback',
      responseType: 'textArea',
      optional: true,
    }],
  },
  showSurveyPanel: true,
});

render(
  <AppStoreContext.Provider value={store}>
    <ThemeProvider theme="auto">
      <SurveyPanel
        onClose={() => process.exit(2)}
        onSubmit={(answers) => {
          writeFileSync(process.env.RESULT_PATH, JSON.stringify(answers));
          setTimeout(() => process.exit(0), 25);
        }}
      />
    </ThemeProvider>
  </AppStoreContext.Provider>
);
`
      );

      try {
        const command = [
          `RESULT_PATH=${shellQuote(resultPath)}`,
          `KIRO_HOME=${shellQuote(dir)}`,
          'KIRO_TERMINAL_THEME=safe',
          'bun',
          shellQuote(fixturePath),
          `2>${shellQuote(fixtureLogPath)}`,
        ].join(' ');
        const started = tmux(socket, [
          '-f',
          '/dev/null',
          'new-session',
          '-d',
          '-s',
          'survey-input',
          '-x',
          '120',
          '-y',
          '40',
          command,
        ]);
        expect(started.status, started.stderr).toBe(0);

        let fixtureOutput = '';
        try {
          await waitFor(() => {
            const captured = tmux(socket, [
              'capture-pane',
              '-p',
              '-J',
              '-t',
              'survey-input:0.0',
            ]);
            fixtureOutput = captured.stdout + captured.stderr;
            return captured.stdout.includes('Type feedback');
          });
        } catch {
          const fixtureLog = existsSync(fixtureLogPath)
            ? readFileSync(fixtureLogPath, 'utf8')
            : '';
          throw new Error(
            `Survey fixture did not paint:\n${fixtureOutput}\n${fixtureLog}`
          );
        }

        const typed = tmux(socket, [
          'send-keys',
          '-t',
          'survey-input:0.0',
          '-l',
          feedback,
        ]);
        expect(typed.status, typed.stderr).toBe(0);
        const submitted = tmux(socket, [
          'send-keys',
          '-t',
          'survey-input:0.0',
          'Enter',
        ]);
        expect(submitted.status, submitted.stderr).toBe(0);

        await waitFor(() => existsSync(resultPath));
        expect(JSON.parse(readFileSync(resultPath, 'utf8'))).toEqual({
          feedback,
        });
      } finally {
        tmux(socket, ['kill-server']);
        rmSync(dir, { recursive: true, force: true });
        rmSync(fixtureDir, { recursive: true, force: true });
      }
    },
    15000
  );
});
