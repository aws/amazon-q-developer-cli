import { Theme } from './types.js';

export const kiroLight: Theme = {
  colors: {
    primary: { truecolor: '#000000', color256: 0, named: 'black' },
    secondary: { truecolor: '#9E9E9E', color256: 247 },
    muted: { truecolor: '#BCBCBC', color256: 250 },
    link: { named: 'blue' },
    accent: { truecolor: '#ff00ff', color256: 13 }, //
    border: { truecolor: '#dadada', color256: 253 }, //
    info: { named: 'cyan' },
    success: { named: 'green' },
    warning: { named: 'yellow' },
    error: { named: 'red' },
    brand: { truecolor: '#8700FF', color256: 93 },
    brandMuted: { truecolor: '#C19AFF', color256: 141 },
    components: {
      snackbar: {
        background: { truecolor: '#552B99', color256: 55 }, // Always dark purple (same as dark mode)
        text: { truecolor: '#ffffff', color256: 15 }, // Always bright purple (same as dark mode)
      },
    },
    syntax: {
      keyword: { truecolor: '#9370d1', color256: 134 }, // keyword.control - darker purple (from #C2A0FD)
      built_in: { truecolor: '#4dc2d9', color256: 74 }, // built-in objects - darker cyan (from #80F4FF)
      string: { truecolor: '#4dd68a', color256: 77 }, // string - darker green (from #80FFB5)
      comment: { truecolor: '#d9d9d9', color256: 253 }, // comment - darker gray (from #FFFFFF99)
      number: { truecolor: '#d982ad', color256: 175 }, // constant.numeric - darker pink (from #FFAFD1)
      literal: { truecolor: '#d94d4d', color256: 167 }, // constant.language.boolean - darker red (from #FF8080)
      regexp: { truecolor: '#4dd68a', color256: 77 }, // string (regex uses string color) (from #80FFB5)
      function: { truecolor: '#5aa3d9', color256: 74 }, // entity.name.function - darker cyan (from #8DC8FB)
      class: { truecolor: '#d94d8a', color256: 168 }, // support.class - darker pink (from #FF80B5)
      type: { truecolor: '#d9d9d9', color256: 253 }, // type annotations - darker gray (from #FFFFFFCC)
      title: { truecolor: '#5aa3d9', color256: 74 }, // entity.name.function - darker cyan (from #8DC8FB)
      name: { truecolor: '#d9d9d9', color256: 253 }, // names - darker gray (from #FFFFFFCC)
      params: { truecolor: '#808080', color256: 244 }, // punctuation - darker gray (from #FFFFFFCC)
      variable: { truecolor: '#4dc2d9', color256: 74 }, // variable - darker cyan (from #80F4FF)
      attr: { truecolor: '#5aa3d9', color256: 74 }, // attributes/properties - darker cyan (from #8DC8FB)
      punctuation: { truecolor: '#808080', color256: 244 }, // punctuation.definition - darker gray (from #FFFFFFCC)
      property: { truecolor: '#5aa3d9', color256: 74 }, // object properties - darker cyan (from #8DC8FB)
      operator: { truecolor: '#808080', color256: 244 }, // keyword.operator - darker gray (from #FFFFFFCC)
      subst: { truecolor: '#000000', color256: 0 }, // parsed sections in strings - black for light theme (from #FFFFFF)
    },
    diff: {
      added: {
        background: { truecolor: '#ffffff', color256: 15 }, // diffEditor.insertedLineBackground - light green
        bar: { truecolor: '#5de89d', color256: 78 }, // editorGutter.addedBackground - darker green
        highlight: { truecolor: '#cffffff', color256: 15 }, // diffEditor.insertedTextBackground - medium green
      },
      removed: {
        background: { truecolor: '#f0d4d4', color256: 224 }, // diffEditor.removedLineBackground - light red
        bar: { truecolor: '#eb5c5c', color256: 203 }, // editorGutter.deletedBackground - darker red
        highlight: { truecolor: '#e8c2c2', color256: 217 }, // diffEditor.removedTextBackground - medium red
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
