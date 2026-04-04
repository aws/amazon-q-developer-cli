/**
 * A "safe" theme that uses ANSI named colors instead of hardcoded truecolor values.
 * Named colors are mapped by the terminal emulator to its own palette, so they
 * adapt naturally to both light and dark backgrounds.
 *
 * Used as the fallback when theme detection fails (e.g., SSH into a headless
 * Linux server where OSC 11 doesn't work and no env vars are available).
 */
import type { Theme } from './types.js';

export const kiroSafe: Theme = {
  colors: {
    primary: { named: 'default' },
    secondary: { named: 'gray' },
    muted: { named: 'gray' },
    surface: { named: 'default' },
    link: { named: 'blue' },
    accent: { named: 'magenta' },
    info: { named: 'cyan' },
    success: { named: 'green' },
    warning: { named: 'yellow' },
    error: { named: 'red' },
    brand: { named: 'magentaBright' },
    brandMuted: { named: 'magenta' },
    highlight: { named: 'blueBright' },
    components: {
      snackbar: {
        background: { named: 'magenta' },
        text: { named: 'white' },
      },
    },
    syntax: {
      keyword: { named: 'magenta' },
      built_in: { named: 'cyan' },
      string: { named: 'green' },
      comment: { named: 'gray' },
      number: { named: 'magentaBright' },
      literal: { named: 'red' },
      regexp: { named: 'green' },
      function: { named: 'blue' },
      class: { named: 'magentaBright' },
      type: { named: 'default' },
      title: { named: 'blue' },
      name: { named: 'default' },
      params: { named: 'default' },
      variable: { named: 'cyan' },
      attr: { named: 'blue' },
      punctuation: { named: 'default' },
      property: { named: 'blue' },
      operator: { named: 'default' },
      subst: { named: 'default' },
    },
    diff: {
      added: {
        background: { named: 'default' },
        bar: { named: 'green' },
        highlight: { named: 'default' },
      },
      removed: {
        background: { named: 'default' },
        bar: { named: 'red' },
        highlight: { named: 'default' },
      },
      unchanged: {
        bar: { named: 'gray' },
      },
    },
  },
  textStyles: {
    label: {
      color: 'primary',
    },
    selectedLabel: {
      color: 'accent',
      weight: 'bold',
    },
  },
};
