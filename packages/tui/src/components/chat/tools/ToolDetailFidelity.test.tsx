import React from 'react';
import { describe, expect, test } from 'vitest';
import { renderWithProviders } from './__tests__/twinki-render.js';
import { Code } from './Code.js';
import { Glob } from './Glob.js';
import { Grep } from './Grep.js';
import { ImageRead } from './ImageRead.js';
import { Read } from './Read.js';
import { SessionTool } from './SessionTool.js';
import { Shell } from './Shell.js';
import { Tool } from './Tool.js';
import { Write } from './Write.js';

describe('tool detail fidelity', () => {
  test.each([
    [
      'code target',
      <Code
        noStatusBar
        isFinished
        content={JSON.stringify({
          operation: 'get_document_symbols',
          file_path: 'C:\\workspace\\src\\App.tsx',
        })}
      />,
      'Code App.tsx',
    ],
    [
      'generic location',
      <Tool
        name="Inspect"
        noStatusBar
        isFinished
        locations={[{ path: 'C:\\workspace\\src\\App.tsx', line: 12 }]}
      />,
      'App.tsx:12',
    ],
    [
      'glob result',
      <Glob
        noStatusBar
        isFinished
        content={JSON.stringify({ pattern: '**/*.tsx' })}
        result={{
          status: 'success',
          output: {
            filePaths: ['C:\\workspace\\src\\App.tsx'],
            totalFiles: 1,
            truncated: false,
          },
        }}
      />,
      'App.tsx',
    ],
    [
      'grep result',
      <Grep
        noStatusBar
        isFinished
        content={JSON.stringify({ pattern: 'release' })}
        result={{
          status: 'success',
          output: {
            numMatches: 1,
            numFiles: 1,
            truncated: false,
            results: [
              {
                file: 'C:\\workspace\\src\\App.tsx',
                count: 1,
                matches: ['12:release'],
              },
            ],
          },
        }}
      />,
      'App.tsx',
    ],
    [
      'multi-file read',
      <Read
        noStatusBar
        isFinished
        content={JSON.stringify({
          paths: [
            'C:\\workspace\\src\\App.tsx',
            'C:\\workspace\\src\\Store.ts',
          ],
        })}
      />,
      'Store.ts',
    ],
    [
      'image target',
      <ImageRead
        noStatusBar
        isFinished
        content={JSON.stringify({
          paths: ['C:\\workspace\\assets\\diagram.png'],
        })}
      />,
      'diagram.png',
    ],
    [
      'write summary',
      <Write
        isFinished
        content={JSON.stringify({
          command: 'strReplace',
          path: 'C:\\workspace\\src\\App.tsx',
          oldStr: 'before',
          newStr: 'after',
        })}
      />,
      'in App.tsx',
    ],
  ])('formats Windows paths in the %s', async (_name, element, expected) => {
    expect(await renderWithProviders(element)).toContain(expected);
  });

  test('preserves backend timeout detail', async () => {
    const output = await renderWithProviders(
      <Shell
        name="Bash"
        command="sleep 60"
        noStatusBar
        isFinished
        result={{
          status: 'error',
          error: 'Process timed out after 30 seconds',
        }}
      />
    );

    expect(output).toContain('Process timed out after 30 seconds');
  });

  test('uses the backend session command for action labels', async () => {
    const output = await renderWithProviders(
      <SessionTool
        name="session_management"
        isFinished
        content={JSON.stringify({
          command: 'get_session_status',
          target: 'release-worker',
        })}
        result={{ status: 'success', output: '' }}
      />
    );

    expect(output).toContain('Checked session');
    expect(output).toContain('release-worker');
    expect(output).not.toContain('Used session tool');
  });
});
