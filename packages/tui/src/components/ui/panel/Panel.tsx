import React, { useRef, useState } from 'react';
import { Box, useInput } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { Divider } from '../divider/Divider.js';
import { Text } from '../text/Text.js';

export interface PanelProps {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  onTabSwitch?: () => void;
  showTabHint?: boolean;
  tabHintLabel?: string;
  footerExtra?: React.ReactNode;
  footerLeft?: React.ReactNode;
  hideTitleDivider?: boolean;
  searchable?: boolean;
  onSearchChange?: (search: string) => void;
  canScrollUp?: boolean;
  canScrollDown?: boolean;
  onScrollUp?: () => void;
  onScrollDown?: () => void;
  /**
   * Verb shown next to the close shortcut in the footer (default: 'to close').
   * Use to disambiguate when Esc means "go back" rather than "close everything"
   * — the underlying keypress handler is unchanged; this only labels intent.
   */
  closeHintLabel?: string;
  /**
   * Spaces prepended to the footer hints so they line up with indented
   * content (e.g. a menu's chevron gutter). Default 0: flush left.
   */
  footerIndent?: number;
}

export const Panel: React.FC<PanelProps> = ({
  title,
  children,
  onClose,
  onTabSwitch,
  showTabHint = false,
  tabHintLabel = 'to switch view',
  footerExtra,
  footerLeft,
  hideTitleDivider = false,
  searchable = false,
  onSearchChange,
  canScrollUp = false,
  canScrollDown = false,
  onScrollUp,
  onScrollDown,
  closeHintLabel = 'to close',
  footerIndent = 0,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const keybindings = useKeybindings();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const glyphs = useGlyphs();

  const [search, setSearch] = useState('');
  // Mirror search into a ref so the useInput closure always sees the latest
  // value. Twinki's useInput handler holds a ref to the latest handler, but
  // the handler itself still closes over the render-time `search` value —
  // when the user mashes Esc fast, the second keystroke can fire before the
  // re-render that cleared the search has committed, and the stale closure
  // takes the clear branch a second time. Reading from a ref bypasses that
  // (the ref is mutated synchronously below).
  const searchRef = useRef(search);
  searchRef.current = search;

  useInput((_input, key) => {
    if (keybindings.matches('closeMenu', _input, key)) {
      if (searchable && searchRef.current.length > 0) {
        searchRef.current = '';
        setSearch('');
        onSearchChange?.('');
      } else {
        onClose();
      }
      return;
    }
    if (key.tab && onTabSwitch) {
      onTabSwitch();
      return;
    }
    if (key.upArrow) {
      onScrollUp?.();
      return;
    }
    if (key.downArrow) {
      onScrollDown?.();
      return;
    }
    if (searchable) {
      if (key.backspace || key.delete) {
        const next = search.slice(0, -1);
        setSearch(next);
        onSearchChange?.(next);
      } else if (
        _input &&
        _input.length === 1 &&
        _input >= ' ' &&
        !key.ctrl &&
        !key.meta
      ) {
        const next = search + _input;
        setSearch(next);
        onSearchChange?.(next);
      }
    }
  });

  return (
    <Box flexDirection="column" width={termWidth}>
      <Box paddingX={1}>
        <Text>{primary(title)}</Text>
      </Box>
      {!hideTitleDivider && <Divider />}

      {searchable && (
        <Box paddingX={1} marginBottom={1}>
          <Text>{dim('search: ')}</Text>
          {search ? <Text>{primary(search)}</Text> : null}
          <Text inverse> </Text>
          {!search && <Text>{dim(' type to filter')}</Text>}
        </Box>
      )}

      <Box flexDirection="column" paddingX={1}>
        {canScrollUp && <Text>{dim(`  ${glyphs.arrowUp} more`)}</Text>}
        {children}
        {canScrollDown && <Text>{dim(`  ${glyphs.arrowDown} more`)}</Text>}
      </Box>

      <Divider />
      <Box justifyContent="space-between" paddingX={1}>
        <Box>
          <Text>
            {footerIndent > 0 ? ' '.repeat(footerIndent) : ''}
            {primary(keybindings.label('closeMenu'))}{' '}
            {dim(searchable && search ? 'to clear search' : closeHintLabel)}
            {canScrollUp || canScrollDown
              ? dim(
                  ` ${glyphs.smallDot} ${glyphs.arrowUp}${glyphs.arrowDown} to scroll`
                )
              : ''}
          </Text>
          {footerLeft && <Text>{dim(` ${glyphs.smallDot} `)}</Text>}
          {footerLeft}
        </Box>
        <Box>
          {footerExtra}
          {footerExtra && showTabHint && (
            <Text>{dim(` ${glyphs.smallDot} `)}</Text>
          )}
          {showTabHint && (
            <Text>
              {primary('Tab')} {dim(tabHintLabel)}
            </Text>
          )}
        </Box>
      </Box>
    </Box>
  );
};
