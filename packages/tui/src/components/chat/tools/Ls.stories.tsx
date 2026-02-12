import { Ls } from './Ls.js';

const meta = {
  component: Ls,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Listing',
      'Listed',
      'ManyEntries',
      'Error',
      'Standalone',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Listing state (in progress)
export const Listing = {
  args: {
    content: JSON.stringify({ path: 'src/components' }),
    isFinished: false,
  },
};

// Listed with a few entries
export const Listed = {
  args: {
    content: JSON.stringify({ path: 'src/components' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'User id: 501\ndrwxr-xr-x  staff  160  Jan 15 10:30  Button\ndrwxr-xr-x  staff  128  Jan 14 09:15  Modal\n-rw-r--r--  staff  2048  Jan 13 14:22  index.ts',
        }],
      },
    },
  },
};

// Many entries (triggers collapse)
export const ManyEntries = {
  args: {
    content: JSON.stringify({ path: 'src', depth: 1 }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'User id: 501\ndrwxr-xr-x  staff  256  Feb 10 11:00  components\ndrwxr-xr-x  staff  192  Feb 09 16:30  hooks\ndrwxr-xr-x  staff  128  Feb 08 09:45  utils\ndrwxr-xr-x  staff  160  Feb 07 14:20  stores\ndrwxr-xr-x  staff  96   Feb 06 10:15  types\n-rw-r--r--  staff  4096  Feb 05 08:30  index.tsx\n-rw-r--r--  staff  1024  Feb 04 17:00  App.tsx\n-rw-r--r--  staff  512   Feb 03 12:45  kiro.ts',
        }],
      },
    },
  },
};

// Error state
export const Error = {
  args: {
    content: JSON.stringify({ path: '/nonexistent/dir' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Directory not found: /nonexistent/dir',
    },
  },
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    content: JSON.stringify({ path: 'packages/tui' }),
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'User id: 501\ndrwxr-xr-x  staff  256  Feb 10 11:00  src\n-rw-r--r--  staff  1024  Feb 09 16:30  package.json\n-rw-r--r--  staff  512   Feb 08 09:45  tsconfig.json',
        }],
      },
    },
  },
};
