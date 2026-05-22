import React, { useState } from 'react';
import { Box, useInput } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
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
}) => {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const keybindings = useKeybindings();
  const primary = getColor('primary');
  const dim = getColor('secondary');

  const [search, setSearch] = useState('');

  useInput((_input, key) => {
    if (keybindings.matches('closeMenu', _input, key)) {
      if (searchable && search) {
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
        {canScrollUp && <Text>{dim('  ↑ more')}</Text>}
        {children}
        {canScrollDown && <Text>{dim('  ↓ more')}</Text>}
      </Box>

      <Divider />
      <Box justifyContent="space-between" paddingX={1}>
        <Box>
          <Text>
            {primary(keybindings.label('closeMenu').toUpperCase())}{' '}
            {dim(searchable && search ? 'to clear search' : closeHintLabel)}
            {canScrollUp || canScrollDown ? dim(' · ↑↓ to scroll') : ''}
          </Text>
          {footerLeft && <Text>{dim(' | ')}</Text>}
          {footerLeft}
        </Box>
        <Box>
          {footerExtra}
          {footerExtra && showTabHint && <Text>{dim(' | ')}</Text>}
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
