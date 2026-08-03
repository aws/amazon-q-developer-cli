/**
 * Renderer — re-exports from twinki.
 * All components should import from here instead of directly from 'twinki'.
 */

export {
  Box,
  Text,
  Static,
  Newline,
  Spacer,
  Transform,
  Region,
  Scrollbar,
  StreamingPanel,
  Split,
  Tabs,
  Input,
  useTwinkiContext,
  useInput,
  useApp,
  useStdin,
  useStdout,
  useFocus,
  useFocusManager,
  useMouse,
  usePaste,
  useFullscreen,
  useTabs,
  render,
  measureElement,
  CURSOR_MARKER,
  useHardwareCursor,
} from 'twinki';

export type {
  TextProps,
  BoxProps,
  SplitProps,
  Tab,
  TabsProps,
  UseTabsOpts,
  TabsModel,
} from 'twinki';
export type { InkKey as Key } from 'twinki';
