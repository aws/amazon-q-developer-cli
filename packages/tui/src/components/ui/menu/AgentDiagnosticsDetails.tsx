import React, { useState, useCallback, useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import type { UpgradeAnalysisRow } from '../../../stores/app-store.js';

export interface AgentDiagnosticsDetailsProps {
  row: UpgradeAnalysisRow;
  onBack: () => void;
  /** Max content lines visible before showing (+N more). */
  visibleLines?: number;
}

const REGEX_KINDS = new Set([
  'regex-shell-pattern',
  'regex-web-pattern',
  'unconvertible-pattern',
]);

interface WarningCopy {
  title: string;
  attribute: string;
  change: string;
  mitigation: string;
}

/** Attribute / what-changed / mitigation copy for the non-regex warnings. */
const NONREGEX_COPY: Record<string, WarningCopy> = {
  'deprecated-aws-tool': {
    title: 'Deprecated AWS tool',
    attribute: 'tools / allowedTools',
    change: 'Deprecated in V3',
    mitigation: 'Use an AWS MCP server',
  },
  'unmapped-allowed-tool': {
    title: 'Unsupported tool',
    attribute: 'allowedTools',
    change: 'No V3 equivalent — ignored',
    mitigation: 'Switch to a V3-supported tool',
  },
  'file-prompt': {
    title: 'Absolute file:// prompt',
    attribute: 'prompt',
    change: 'Absolute file:// URI — V3 rejects it',
    mitigation: 'Use a workspace-relative path',
  },
  'deny-by-default-readonly': {
    title: 'Deny-by-default vs auto read-only',
    attribute: 'toolsSettings.shell',
    change: "V3 can't combine them — read-only auto-approval dropped",
    mitigation:
      'Remove denyByDefault, or add read-only commands to allowedCommands',
  },
};

function truncate(s: string, n = 120): string {
  return s.length > n ? s.slice(0, n - 1) + '…' : s;
}

/** Conversion status of a grouped set of regex warnings. */
type StatusCat = 'approximate' | 'unmapped' | 'denied-all';

const REGEX_COPY: Record<
  StatusCat,
  { title: string; change: string; mitigation: string }
> = {
  approximate: {
    title: 'Approximate match',
    change: 'V2 regex converted to V3 glob(s) — no exact equivalent',
    mitigation: 'Verify the glob(s); update if needed',
  },
  unmapped: {
    title: 'Not convertible — left unmapped',
    change: 'Regex has no glob form (lookaround / chaining / too complex)',
    mitigation: 'Add an equivalent glob rule manually',
  },
  'denied-all': {
    title: 'Not convertible — denied ALL to stay safe',
    change: 'Deny regex has no glob form; every command is denied',
    mitigation: 'Narrow the deny rule manually',
  },
};

/**
 * Full-height detail for one agent in `/upgrade-agent diagnostics`. Mirrors
 * PromptDetails' scroll/keypress shape (esc = back, ↑↓ = scroll). Non-regex
 * warnings render as Attribute / What changed / Mitigation; regex conversions
 * are grouped under their source field (e.g. toolsSettings.shell.allowedCommands)
 * showing each regex → glob with its lossy/unconvertible mitigation.
 */
export const AgentDiagnosticsDetails: React.FC<
  AgentDiagnosticsDetailsProps
> = ({ row, onBack, visibleLines = 12 }) => {
  const { getColor } = useTheme();
  const dimText = getColor('secondary');
  const brandText = getColor('primary');
  const glyphs = useGlyphs();
  const [scrollOffset, setScrollOffset] = useState(0);

  const lines = useMemo<React.ReactNode[]>(() => {
    const field = (k: string, v: string) => (
      <Text>
        {dimText(`${k}:`.padEnd(14))}
        {v}
      </Text>
    );

    const out: React.ReactNode[] = [];

    if (row.warnings.length === 0) {
      out.push(
        <Box key="ok">
          <Text>
            {dimText(
              'No conversion warnings — nothing to review for this agent.'
            )}
          </Text>
        </Box>
      );
    }

    // --- non-regex warnings: Attribute / What changed / Mitigation ---
    const byKind = new Map<string, string[]>();
    for (const w of row.warnings) {
      if (REGEX_KINDS.has(w.kind)) continue;
      const arr = byKind.get(w.kind) ?? [];
      if (w.detail) arr.push(w.detail);
      byKind.set(w.kind, arr);
    }
    for (const [kind, details] of byKind) {
      const copy = NONREGEX_COPY[kind];
      out.push(
        <Box key={`t-${kind}`}>
          <Text>{brandText(copy?.title ?? kind)}</Text>
        </Box>
      );
      out.push(
        <Box key={`a-${kind}`} paddingLeft={2}>
          {field('Attribute', copy?.attribute ?? kind)}
        </Box>
      );
      out.push(
        <Box key={`c-${kind}`} paddingLeft={2}>
          {field('What changed', copy?.change ?? '')}
        </Box>
      );
      out.push(
        <Box key={`m-${kind}`} paddingLeft={2}>
          {field('Mitigation', copy?.mitigation ?? '')}
        </Box>
      );
      if (details.length) {
        const counts = new Map<string, number>();
        for (const d of details) counts.set(d, (counts.get(d) ?? 0) + 1);
        const rendered = Array.from(counts, ([v, c]) =>
          c > 1 ? `${v} ×${c}` : v
        ).join(', ');
        out.push(
          <Box key={`v-${kind}`} paddingLeft={2}>
            {field('Affected', rendered)}
          </Box>
        );
      }
      out.push(<Box key={`sp-${kind}`} height={1} />);
    }

    // --- regex warnings: grouped by source field + conversion status ---
    const regexGroups = new Map<
      string,
      {
        attr: string;
        isDeny: boolean;
        status: StatusCat;
        ws: typeof row.warnings;
      }
    >();
    for (const w of row.warnings) {
      if (!REGEX_KINDS.has(w.kind)) continue;
      const attr = w.attribute ?? 'toolsSettings (regex)';
      const isDeny = w.effect === 'deny';
      const status: StatusCat =
        w.kind !== 'unconvertible-pattern'
          ? 'approximate'
          : isDeny
            ? 'denied-all'
            : 'unmapped';
      const key = `${attr}::${status}`;
      const g = regexGroups.get(key) ?? { attr, isDeny, status, ws: [] };
      g.ws.push(w);
      regexGroups.set(key, g);
    }

    for (const { attr, isDeny, status, ws } of regexGroups.values()) {
      const copy = REGEX_COPY[status];
      out.push(
        <Box key={`rh-${attr}-${status}`}>
          <Text>{brandText(copy.title)}</Text>
        </Box>
      );
      out.push(
        <Box key={`ra-${attr}-${status}`} paddingLeft={2}>
          {field(
            'Attribute',
            `${attr} → permissions.rules (${isDeny ? 'deny' : 'allow'})`
          )}
        </Box>
      );
      out.push(
        <Box key={`rc-${attr}-${status}`} paddingLeft={2}>
          {field('What changed', copy.change)}
        </Box>
      );
      out.push(
        <Box key={`rm-${attr}-${status}`} paddingLeft={2}>
          {field('Mitigation', copy.mitigation)}
        </Box>
      );
      out.push(
        <Box key={`rpl-${attr}-${status}`} paddingLeft={2}>
          <Text>{dimText('Patterns:')}</Text>
        </Box>
      );
      ws.forEach((w, i) => {
        let rowText: string;
        if (status === 'approximate') {
          const gl = w.converted ?? [];
          const preview = gl.slice(0, 2).join(', ');
          const more = gl.length > 2 ? ` (+${gl.length - 2} more)` : '';
          rowText = `${truncate(w.detail ?? '', 46)}  →  ${truncate(preview, 32)}${more}`;
        } else {
          rowText = truncate(w.detail ?? '', 88);
        }
        out.push(
          <Box key={`rr-${attr}-${status}-${i}`} paddingLeft={4}>
            <Text>
              {dimText(`${glyphs.smallDot} `)}
              {rowText}
            </Text>
          </Box>
        );
      });
      out.push(<Box key={`rgap-${attr}-${status}`} height={1} />);
    }

    return out;
  }, [row, dimText, brandText, glyphs]);

  const maxOffset = Math.max(0, lines.length - visibleLines);

  useKeypress(
    useCallback(
      (
        _input: string,
        key: { escape: boolean; upArrow: boolean; downArrow: boolean }
      ) => {
        if (key.escape) onBack();
        else if (key.upArrow) setScrollOffset((p) => Math.max(0, p - 1));
        else if (key.downArrow)
          setScrollOffset((p) => Math.min(maxOffset, p + 1));
      },
      [onBack, maxOffset]
    )
  );

  const visibleSlice = lines.slice(scrollOffset, scrollOffset + visibleLines);
  const remaining = lines.length - scrollOffset - visibleSlice.length;

  return (
    <Box flexDirection="column">
      <Box>
        <Text>
          {dimText('Agent: ')}
          {brandText(row.name)}
          {dimText(
            ` ${glyphs.smallDot} ${row.scope === 'local' ? 'Workspace' : 'Global'} ${glyphs.smallDot} `
          )}
          {'Universal'}
        </Text>
      </Box>
      <Box height={1} />
      <Box
        flexDirection="column"
        paddingLeft={1}
        height={visibleLines}
        overflow="hidden"
      >
        {visibleSlice}
      </Box>
      {remaining > 0 ? (
        <Box paddingLeft={1}>
          <Text>{dimText(`(+${remaining} more)`)}</Text>
        </Box>
      ) : (
        <Box height={1} />
      )}
      <Divider />
      <Box paddingX={1}>
        <Text>
          {brandText('esc')} {dimText('to go back')}
          {dimText(` ${glyphs.smallDot} `)}
          {brandText(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
          {dimText('to scroll')}
        </Text>
      </Box>
    </Box>
  );
};
