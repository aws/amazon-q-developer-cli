import { describe, it, expect } from 'bun:test';

// We need to test buildPreview which is not exported.
// Extract the logic into a testable function or test via buildTurnList.
// For now, inline the logic here for unit testing.

const MessageRole = {
  User: 'user',
  Model: 'model',
  ToolUse: 'tool_use',
  System: 'system',
} as const;
const MAX_PREVIEW_LINES = 40;
const MAX_LINE_WIDTH = 160;
const TOOL_PREFIX = '↳ ';

function buildPreview(
  msgs: Array<{ role: string; content: string; name?: string }>
): string {
  const hasTools = msgs.some((m) => m.role === MessageRole.ToolUse);
  if (!hasTools) {
    const model = msgs.find((m) => m.role === MessageRole.Model);
    if (!model) return '';
    const line = model.content.split('\n').find((l) => l.trim());
    return line ? truncate(line) : '';
  }

  const lines: string[] = [];
  for (const msg of msgs) {
    if (lines.length >= MAX_PREVIEW_LINES) break;
    if (msg.role === MessageRole.Model && msg.content) {
      const line = msg.content.split('\n').find((l) => l.trim());
      if (line) lines.push(truncate(line));
    } else if (msg.role === MessageRole.ToolUse) {
      lines.push(TOOL_PREFIX + toolLabel(msg.name || 'unknown', msg.content));
    }
  }
  return lines.join('\n');
}

function toolLabel(name: string, contentJson: string): string {
  try {
    const args = JSON.parse(contentJson);
    const purpose = args?.__tool_use_purpose?.trim();
    if (purpose)
      return `${name}: ${purpose[0]?.toLowerCase()}${purpose.slice(1)}`;
    const keyArg = extractKeyArg(name, args);
    if (keyArg) return `${name} ${keyArg}`;
  } catch {
    /* malformed */
  }
  return name;
}

function extractKeyArg(name: string, args: Record<string, unknown>): string {
  const paths = args.paths as string[] | undefined;
  if (Array.isArray(paths) && paths.length > 0) {
    const first = paths[0]!.split('/').pop() || paths[0]!;
    return paths.length > 1 ? `${first} +${paths.length - 1} more` : first;
  }

  const val = (args.command ??
    args.path ??
    args.filePath ??
    args.file_path ??
    args.pattern ??
    args.query ??
    args.url ??
    null) as string | null;
  if (typeof val !== 'string') return '';
  const short =
    val.includes('/') && !val.includes(' ') ? val.split('/').pop() || val : val;
  return truncate(short);
}

function truncate(s: string): string {
  return s.length > MAX_LINE_WIDTH ? s.slice(0, MAX_LINE_WIDTH - 1) + '…' : s;
}

function elideLines(lines: string[], max: number): string[] {
  if (max <= 0) return [];
  if (lines.length <= max) return lines;
  if (max === 1) return [`⋯ ${lines.length} more ⋯`];
  const visible = max - 1;
  const head = Math.ceil(visible / 2);
  const tail = visible - head;
  const hidden = lines.length - head - tail;
  const firstHidden = lines[head] || '';
  const indent = firstHidden.match(/^(\s*)/)?.[1] || '';
  return [
    ...lines.slice(0, head),
    `${indent}⋯ ${hidden} more ⋯`,
    ...(tail > 0 ? lines.slice(lines.length - tail) : []),
  ];
}

describe('rewind preview', () => {
  describe('buildPreview', () => {
    it('case 1: 1 message + 5 tool calls — fits, show all', () => {
      const msgs = [
        {
          role: MessageRole.Model,
          content: 'Let me run ls five times',
          name: undefined,
        },
        ...Array.from({ length: 5 }, () => ({
          role: MessageRole.ToolUse,
          content: JSON.stringify({ command: 'ls' }),
          name: 'Run Command',
        })),
      ];
      const result = buildPreview(msgs);
      const lines = result.split('\n');
      expect(lines[0]).toBe('Let me run ls five times');
      expect(lines[1]).toBe('↳ Run Command ls');
      expect(lines.length).toBe(6); // 1 text + 5 tools
    });

    it('case 2: 2 messages + 8 tool calls — uses elision', () => {
      const msgs = [
        {
          role: MessageRole.Model,
          content: 'Let me check everything',
          name: undefined,
        },
        ...Array.from({ length: 8 }, (_, i) => ({
          role: MessageRole.ToolUse,
          content: JSON.stringify({ path: `src/file${i}.ts` }),
          name: 'Read File',
        })),
        {
          role: MessageRole.Model,
          content: 'Done, all passing',
          name: undefined,
        },
      ];
      const result = buildPreview(msgs);
      const lines = result.split('\n');
      expect(lines.length).toBe(10); // 2 text + 8 tools
      // After elision to 8 lines:
      const elided = elideLines(lines, 8);
      expect(elided.length).toBe(8);
      expect(elided[0]).toBe('Let me check everything');
      expect(elided[elided.length - 1]).toBe('Done, all passing');
      expect(elided.some((l) => l.includes('⋯'))).toBe(true);
    });

    it('case 3: 0 messages + 12 tool calls — pure tool spam', () => {
      const msgs = Array.from({ length: 12 }, (_, i) => ({
        role: MessageRole.ToolUse,
        content: JSON.stringify({
          path: `src/${String.fromCharCode(97 + i)}.ts`,
        }),
        name: 'Read File',
      }));
      const result = buildPreview(msgs);
      const lines = result.split('\n');
      expect(lines.length).toBe(12);
      expect(lines[0]).toBe('↳ Read File a.ts');
      // Elided:
      const elided = elideLines(lines, 8);
      expect(elided.length).toBe(8);
      expect(elided[0]).toBe('↳ Read File a.ts');
      expect(elided[elided.length - 1]).toBe('↳ Read File l.ts');
      // Marker matches indent of first hidden line (flush-left for ↳ lines)
      const marker = elided.find((l) => l.includes('⋯'))!;
      expect(marker).toBe('⋯ 5 more ⋯');
    });

    it('case 4: 5 messages + 1 tool each — interleaved', () => {
      const msgs: Array<{ role: string; content: string; name?: string }> = [];
      for (let i = 0; i < 5; i++) {
        msgs.push({
          role: MessageRole.Model,
          content: `Step ${i + 1}`,
          name: undefined,
        });
        msgs.push({
          role: MessageRole.ToolUse,
          content: JSON.stringify({ command: `cmd${i}` }),
          name: 'Run Command',
        });
      }
      const result = buildPreview(msgs);
      const lines = result.split('\n');
      expect(lines.length).toBe(10); // 5 text + 5 tools
      expect(lines[0]).toBe('Step 1');
      expect(lines[1]).toBe('↳ Run Command cmd0');
      // Elided to 8:
      const elided = elideLines(lines, 8);
      expect(elided.length).toBe(8);
      expect(elided[0]).toBe('Step 1');
      expect(elided[elided.length - 1]).toBe('↳ Run Command cmd4');
    });

    it('pure text turn — no tools, shows first line of assistant response', () => {
      const msgs = [
        {
          role: MessageRole.Model,
          content: 'Hello! How can I help?',
          name: undefined,
        },
      ];
      expect(buildPreview(msgs)).toBe('Hello! How can I help?');
    });

    it('tool with __tool_use_purpose shows purpose', () => {
      const msgs = [
        {
          role: MessageRole.ToolUse,
          content: JSON.stringify({
            path: 'config.ts',
            __tool_use_purpose: 'Check the timeout setting',
          }),
          name: 'Read File',
        },
      ];
      const result = buildPreview(msgs);
      expect(result).toBe('↳ Read File: check the timeout setting');
    });

    it('tool without purpose shows key arg', () => {
      const msgs = [
        {
          role: MessageRole.ToolUse,
          content: JSON.stringify({ command: 'npm test' }),
          name: 'Run Command',
        },
      ];
      expect(buildPreview(msgs)).toBe('↳ Run Command npm test');
    });

    it('tool with no useful args shows just name', () => {
      const msgs = [
        {
          role: MessageRole.ToolUse,
          content: JSON.stringify({}),
          name: 'User Input',
        },
      ];
      expect(buildPreview(msgs)).toBe('↳ User Input');
    });

    it('multi-file tool shows first file + count', () => {
      const msgs = [
        {
          role: MessageRole.ToolUse,
          content: JSON.stringify({
            paths: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
          }),
          name: 'Read Files',
        },
      ];
      expect(buildPreview(msgs)).toBe('↳ Read Files a.ts +2 more');
    });

    it('single-file paths array shows just the file', () => {
      const msgs = [
        {
          role: MessageRole.ToolUse,
          content: JSON.stringify({ paths: ['src/config.ts'] }),
          name: 'Read Files',
        },
      ];
      expect(buildPreview(msgs)).toBe('↳ Read Files config.ts');
    });
  });

  describe('elideLines', () => {
    it('returns all lines when under cap', () => {
      const lines = ['a', 'b', 'c'];
      expect(elideLines(lines, 5)).toEqual(['a', 'b', 'c']);
    });

    it('elides middle with correct count', () => {
      const lines = Array.from({ length: 10 }, (_, i) => `line${i}`);
      const result = elideLines(lines, 8);
      expect(result.length).toBe(8);
      expect(result.find((l) => l.includes('⋯'))).toBe('⋯ 3 more ⋯');
    });

    it('preserves indent on marker', () => {
      const lines = [
        'text',
        '↳ tool1',
        '↳ tool2',
        '↳ tool3',
        '↳ tool4',
        'text2',
      ];
      const result = elideLines(lines, 4);
      const marker = result.find((l) => l.includes('⋯'))!;
      // First hidden line is '↳ tool2' which has no leading whitespace
      expect(marker).toBe('⋯ 3 more ⋯');
    });
  });

  describe('enrichTurnsWithPreview', () => {
    // Inline a minimal version of the shared function for testing
    function enrichTurnsWithPreview(
      turns: Array<{
        logIndex: number;
        label: string;
        group: string;
        responseSnippet: string;
      }>,
      messages: Array<{
        id: string;
        role: string;
        content: string;
        name?: string;
      }>
    ) {
      // Group by user turn, skip leading non-user
      const groups: (typeof messages)[] = [];
      let current: typeof messages | null = null;
      for (const msg of messages) {
        if (msg.role === MessageRole.User) {
          if (current) groups.push(current);
          current = [msg];
        } else if (current) {
          current.push(msg);
        }
      }
      if (current) groups.push(current);

      const sortedIndices = turns.map((t) => t.logIndex).sort((a, b) => a - b);
      const logIndexToOrdinal = new Map(
        sortedIndices.map((li, ord) => [li, ord])
      );

      return turns.map((turn) => {
        const ordinal = logIndexToOrdinal.get(turn.logIndex);
        const group = ordinal != null ? groups[ordinal] : undefined;
        if (!group || group.length <= 1) return turn;
        const preview = buildPreview(group.slice(1));
        return { ...turn, responseSnippet: preview || turn.responseSnippet };
      });
    }

    it('enriches turns from store, falls back to backend snippet when no match', () => {
      const turns = [
        {
          logIndex: 0,
          label: 'hello',
          group: '',
          responseSnippet: 'backend fallback',
        },
        {
          logIndex: 1,
          label: 'fix tests',
          group: '',
          responseSnippet: 'old snippet',
        },
      ];
      const messages = [
        { id: '1', role: MessageRole.User, content: 'hello' },
        { id: '2', role: MessageRole.Model, content: 'Hi there!' },
        { id: '3', role: MessageRole.User, content: 'fix tests' },
        {
          id: '4',
          role: MessageRole.ToolUse,
          content: JSON.stringify({ command: 'npm test' }),
          name: 'Run Command',
        },
        { id: '5', role: MessageRole.Model, content: 'All passing' },
      ];
      const result = enrichTurnsWithPreview(turns, messages);
      expect(result[0]!.responseSnippet).toBe('Hi there!');
      expect(result[1]!.responseSnippet).toContain('↳ Run Command npm test');
    });

    it('skips leading system messages without misaligning', () => {
      const turns = [
        { logIndex: 0, label: 'hello', group: '', responseSnippet: 'fallback' },
      ];
      const messages = [
        { id: '0', role: MessageRole.System, content: 'Welcome' },
        { id: '1', role: MessageRole.User, content: 'hello' },
        { id: '2', role: MessageRole.Model, content: 'Hi!' },
      ];
      const result = enrichTurnsWithPreview(turns, messages);
      expect(result[0]!.responseSnippet).toBe('Hi!');
    });

    it('returns backend snippet when store has fewer turns', () => {
      const turns = [
        {
          logIndex: 0,
          label: 'old turn',
          group: '',
          responseSnippet: 'from backend',
        },
        {
          logIndex: 1,
          label: 'recent',
          group: '',
          responseSnippet: 'also backend',
        },
      ];
      // Store only has the recent turn (truncated replay)
      const messages = [
        { id: '1', role: MessageRole.User, content: 'recent' },
        { id: '2', role: MessageRole.Model, content: 'Done' },
      ];
      const result = enrichTurnsWithPreview(turns, messages);
      // First turn: store has data (matches turn[0] to group[0])
      expect(result[0]!.responseSnippet).toBe('Done');
      // Second turn: no group[1] exists, falls back
      expect(result[1]!.responseSnippet).toBe('also backend');
    });

    it('aligns correctly when turns are reversed (newest-first, production order)', () => {
      const turns = [
        {
          logIndex: 1,
          label: 'fix tests',
          group: '',
          responseSnippet: 'old snippet',
        },
        {
          logIndex: 0,
          label: 'hello',
          group: '',
          responseSnippet: 'backend fallback',
        },
      ];
      const messages = [
        { id: '1', role: MessageRole.User, content: 'hello' },
        { id: '2', role: MessageRole.Model, content: 'Hi there!' },
        { id: '3', role: MessageRole.User, content: 'fix tests' },
        {
          id: '4',
          role: MessageRole.ToolUse,
          content: JSON.stringify({ command: 'npm test' }),
          name: 'Run Command',
        },
        { id: '5', role: MessageRole.Model, content: 'All passing' },
      ];
      const result = enrichTurnsWithPreview(turns, messages);
      // turns[0] is logIndex:1 (fix tests) — must get 'fix tests' preview, not 'hello'
      expect(result[0]!.responseSnippet).toContain('↳ Run Command npm test');
      // turns[1] is logIndex:0 (hello) — must get 'hello' preview
      expect(result[1]!.responseSnippet).toBe('Hi there!');
    });
  });
});
