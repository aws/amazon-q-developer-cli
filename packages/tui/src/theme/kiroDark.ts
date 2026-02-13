import type { Theme } from './types.js';

export const kiroDark: Theme = {
  colors: {
    primary: { truecolor: '#FFFFFF', color256: 7, named: 'white' },
    secondary: { truecolor: '#808080', color256: 244 }, // also 8
    muted: { truecolor: '#303030', color256: 236 },
    link: { named: 'blue' },
    accent: { truecolor: '#ff00ff', color256: 13 },
    border: { truecolor: '#262626', color256: 235 },
    info: { named: 'cyan' },
    success: { named: 'green' },
    warning: { named: 'yellow' },
    error: { named: 'red' },
    brand: { truecolor: '#C19AFF', color256: 141 },
    brandMuted: { truecolor: '#8700FF', color256: 93 },
    components: {
      snackbar: {
        background: { truecolor: '#552B99', color256: 57 }, // Always dark purple (same as dark mode)
        text: { truecolor: '#ffffff', color256: 15 }, // Always bright purple (same as dark mode)
      },
    },
    syntax: {
      keyword: { truecolor: '#C2A0FD', color256: 183 }, // keyword.control - purple (function, return, etc)
      built_in: { truecolor: '#80F4FF', color256: 123 }, // built-in objects - cyan (console, Object, Array, etc)
      string: { truecolor: '#80FFB5', color256: 121 }, // string - green
      comment: { truecolor: '#FFFFFF99', color256: 250 }, // comment - white with opacity
      number: { truecolor: '#FFAFD1', color256: 218 }, // constant.numeric - pink
      literal: { truecolor: '#FF8080', color256: 210 }, // constant.language.boolean - red (true/false/null)
      regexp: { truecolor: '#80FFB5', color256: 121 }, // string (regex uses string color)
      function: { truecolor: '#8DC8FB', color256: 117 }, // entity.name.function - cyan (log, helloWorld)
      class: { truecolor: '#FF80B5', color256: 211 }, // support.class - pink
      type: { truecolor: '#FFFFFFCC', color256: 255 }, // type annotations - white (void, string, number types)
      title: { truecolor: '#8DC8FB', color256: 117 }, // entity.name.function - cyan
      name: { truecolor: '#FFFFFFCC', color256: 255 }, // names - white (for type names like "string")
      params: { truecolor: '#FFFFFFCC', color256: 255 }, // punctuation - white
      variable: { truecolor: '#80F4FF', color256: 123 }, // variable - cyan
      attr: { truecolor: '#8DC8FB', color256: 117 }, // attributes/properties - cyan (matches function for method calls)
      punctuation: { truecolor: '#FFFFFFCC', color256: 255 }, // punctuation.definition - white (parens, brackets)
      property: { truecolor: '#8DC8FB', color256: 117 }, // object properties - cyan (log in console.log)
      operator: { truecolor: '#FFFFFFCC', color256: 255 }, // keyword.operator - white (+, -, =, etc)
      subst: { truecolor: '#FFFFFF', color256: 255 }, // parsed sections in strings (template literals) - default/white
    },
    diff: {
      added: {
        background: { truecolor: '#2d3a30', color256: 235 }, // diffEditor.insertedLineBackground (blended with bg)
        bar: { truecolor: '#80ffb5', color256: 121 }, // editorGutter.addedBackground
        highlight: { truecolor: '#2d3a30', color256: 236 }, // diffEditor.insertedTextBackground (blended with bg)
      },
      removed: {
        background: { truecolor: '#3a2d2f', color256: 235 }, // diffEditor.removedLineBackground (blended with bg)
        bar: { truecolor: '#ff8080', color256: 210 }, // editorGutter.deletedBackground
        highlight: { truecolor: '#3a2d2f', color256: 236 }, // diffEditor.removedTextBackground (blended with bg)
      },
      unchanged: {
        bar: { truecolor: '#303030', color256: 236 },
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
