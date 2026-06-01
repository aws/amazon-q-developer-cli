import { describe, it, expect } from 'bun:test';

/**
 * Tests for the ops-parsing logic used in Read.tsx.
 *
 * KAS sends read args in shapes that differ from the legacy fs_read tool:
 *   - read_file:  { path, offset?, limit? }        (flat, no operations wrapper)
 *   - read_files: { paths: string[], ... }         (multi-file)
 *   - fs_read:    { operations: [{ mode, path }] }  (legacy/Rust unified tool)
 *
 * This mirrors the parsing branch in Read.tsx so we can verify all shapes
 * produce the expected ReadOp[] without standing up the full ink renderer.
 */

interface ReadOp {
  path: string;
  limit?: number;
  offset?: number;
}

/** Mirrors the ops parsing in Read.tsx */
function parseOps(content: string): ReadOp[] {
  if (!content) return [];
  try {
    const parsed = JSON.parse(content);
    const rawOps = parsed.operations ?? parsed.ops;
    if (Array.isArray(rawOps)) {
      return rawOps.flatMap((op: Record<string, unknown>): ReadOp[] => {
        const mode = op.mode as string | undefined;
        if (mode === 'Directory') return [{ path: (op.path as string) || '' }];
        if (mode === 'Image') {
          const paths = (op.image_paths ?? op.paths) as string[] | undefined;
          return (paths || []).map((p) => ({ path: p }));
        }
        return [
          {
            path: (op.path as string) || '',
            limit: op.limit as number | undefined,
            offset: op.offset as number | undefined,
          },
        ];
      });
    }
    // Flat format: read_file sends { path, offset?, limit? } directly
    if (typeof parsed.path === 'string') {
      return [
        {
          path: parsed.path,
          limit: parsed.limit as number | undefined,
          offset: parsed.offset as number | undefined,
        },
      ];
    }
    // Multi-file format: read_files sends { paths: string[] }
    if (Array.isArray(parsed.paths)) {
      return parsed.paths.map((p: string) => ({ path: p }));
    }
    return [];
  } catch {
    return [];
  }
}

describe('Read ops parsing', () => {
  it('parses KAS read_file flat format', () => {
    const ops = parseOps('{"path":"TESTING.md"}');
    expect(ops).toHaveLength(1);
    expect(ops[0]!.path).toBe('TESTING.md');
  });

  it('parses KAS read_file with offset and limit', () => {
    const ops = parseOps('{"path":"src/main.ts","offset":10,"limit":50}');
    expect(ops).toHaveLength(1);
    expect(ops[0]).toEqual({ path: 'src/main.ts', offset: 10, limit: 50 });
  });

  it('parses KAS read_files multi-file format', () => {
    const ops = parseOps('{"paths":["a.ts","b.ts","c.ts"]}');
    expect(ops).toHaveLength(3);
    expect(ops.map((o) => o.path)).toEqual(['a.ts', 'b.ts', 'c.ts']);
  });

  it('parses legacy fs_read operations array', () => {
    const ops = parseOps(
      '{"operations":[{"mode":"Line","path":"x.ts","limit":20}]}'
    );
    expect(ops).toHaveLength(1);
    expect(ops[0]).toEqual({ path: 'x.ts', limit: 20, offset: undefined });
  });

  it('parses fs_read Image operation with multiple paths', () => {
    const ops = parseOps(
      '{"operations":[{"mode":"Image","image_paths":["a.png","b.png"]}]}'
    );
    expect(ops.map((o) => o.path)).toEqual(['a.png', 'b.png']);
  });

  it('returns empty for unparseable or empty content', () => {
    expect(parseOps('')).toEqual([]);
    expect(parseOps('not json')).toEqual([]);
    expect(parseOps('{"unrelated":true}')).toEqual([]);
  });
});
