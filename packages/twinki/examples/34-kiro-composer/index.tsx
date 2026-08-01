import { readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { render } from 'twinki';
import { ShowcaseClient } from '../32-acp-showcase/acp-client.js';
import { themeById } from '../32-acp-showcase/themes.js';
import { scanWorkspace } from '../32-acp-showcase/workspace.js';
import { App } from './App.js';
import { parseWorkbenchLayout, type LayoutChoice } from './layout.js';
import { BUILTIN_WIDGET_IDS } from './widgets.js';

const here = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(here, '../../../..');
const layoutsDirectory = resolve(here, 'layouts');
const composerSkill = readFileSync(resolve(repositoryRoot, '.kiro/skills/twinki-app-composer/SKILL.md'), 'utf8');

function valuesAfter(args: string[], flag: string): string[] {
  return args.flatMap((value, index) => (value === flag && args[index + 1] ? [args[index + 1]!] : []));
}

function loadLayout(path: string): LayoutChoice {
  try {
    return {
      path,
      source: basename(path),
      spec: parseWorkbenchLayout(JSON.parse(readFileSync(path, 'utf8')) as unknown, BUILTIN_WIDGET_IDS),
    };
  } catch (error) {
    throw new Error(`Cannot load ${path}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function discoverLayouts(extraPath?: string): LayoutChoice[] {
  const layouts = readdirSync(layoutsDirectory)
    .filter((name) => name.endsWith('.json'))
    .sort()
    .map((name) => loadLayout(resolve(layoutsDirectory, name)));
  if (extraPath && !layouts.some((layout) => layout.path === extraPath)) {
    layouts.push(loadLayout(extraPath));
  }
  return layouts;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const valueAfter = (flag: string): string | undefined => {
    const index = args.indexOf(flag);
    return index >= 0 ? args[index + 1] : undefined;
  };
  const cwd = resolve(valueAfter('--cwd') ?? process.cwd());
  const command = valueAfter('--command') ?? 'kiro-cli';
  const engine = valueAfter('--engine') ?? 'v3';
  const layoutValue = valueAfter('--layout');
  const initialLayoutPath = layoutValue
    ? resolve(
        layoutValue.endsWith('.json') || layoutValue.includes('/')
          ? layoutValue
          : resolve(layoutsDirectory, `${layoutValue}.json`)
      )
    : resolve(layoutsDirectory, 'chat.json');
  const layouts = discoverLayouts(initialLayoutPath);
  if (args.includes('--check')) {
    console.log(
      `Valid Kiro Composer layouts (${layouts.length}): ${layouts.map((layout) => layout.spec.title).join(', ')}`
    );
    return;
  }
  const isKiro = basename(command).startsWith('kiro-cli');
  const commandArgs = isKiro
    ? ['acp', '--agent-engine', engine, ...valuesAfter(args, '--agent-arg')]
    : valuesAfter(args, '--agent-arg');
  const clientOptions = {
    command,
    args: commandArgs,
    cwd,
  };
  const createClient = () => new ShowcaseClient(clientOptions);
  const client = createClient();
  const instance = render(
    <App
      client={client}
      createClient={createClient}
      workspaceRoot={cwd}
      initialFiles={scanWorkspace(cwd)}
      initialTheme={themeById(valueAfter('--theme'))}
      engine={isKiro ? `KAS ${engine.toUpperCase()}` : basename(command)}
      layouts={layouts}
      initialLayoutPath={initialLayoutPath}
      loadLayouts={() => discoverLayouts(initialLayoutPath)}
      composerRoot={here}
      composerSkill={composerSkill}
    />,
    {
      fullscreen: true,
      mouse: true,
      textSelection: true,
      exitOnCtrlC: true,
    }
  );
  const stop = () => instance.unmount();
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
  try {
    await instance.waitUntilExit();
  } finally {
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
    await client.close();
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
