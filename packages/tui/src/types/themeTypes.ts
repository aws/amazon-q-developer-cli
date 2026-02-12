// Theme-related type definitions
export type ChalkColorName =
  | 'black'
  | 'red'
  | 'green'
  | 'yellow'
  | 'blue'
  | 'magenta'
  | 'cyan'
  | 'white'
  | 'blackBright'
  | 'redBright'
  | 'greenBright'
  | 'yellowBright'
  | 'blueBright'
  | 'magentaBright'
  | 'cyanBright'
  | 'whiteBright'
  | 'gray'
  | 'grey';

export interface TerminalColor {
  truecolor?: string;
  color256?: number;
  named?: ChalkColorName;
}

export type Weight = 'normal' | 'bold' | 'dim';
export type Style = 'normal' | 'italic';
export type Decoration = 'underline' | 'strikethrough';

export interface TextStyle {
  color: string; // Color path like 'primary', 'accent', etc.
  weight?: Weight;
  style?: Style;
  decoration?: Decoration[];
}

export interface Theme {
  colors: {
    primary: TerminalColor;
    secondary: TerminalColor;
    muted: TerminalColor;
    // background: TerminalColor;
    // backgroundElevated: TerminalColor;
    // backgroundOverlay: TerminalColor;
    link: TerminalColor;
    accent: TerminalColor;
    border: TerminalColor;
    info: TerminalColor;
    success: TerminalColor;
    warning: TerminalColor;
    error: TerminalColor;
    brand: TerminalColor;
    brandMuted: TerminalColor;
    components: {
      snackbar: {
        background: TerminalColor;
        text: TerminalColor;
      };
    };
    syntax: {
      keyword: TerminalColor;
      built_in: TerminalColor;
      string: TerminalColor;
      comment: TerminalColor;
      number: TerminalColor;
      literal: TerminalColor;
      regexp: TerminalColor;
      function: TerminalColor;
      class: TerminalColor;
      type: TerminalColor;
      title: TerminalColor;
      name: TerminalColor;
      params: TerminalColor;
      variable: TerminalColor;
      attr: TerminalColor;
      punctuation: TerminalColor;
      property: TerminalColor;
      operator: TerminalColor;
      subst: TerminalColor;
    };
    diff: {
      added: {
        background: TerminalColor;
        bar: TerminalColor;
        highlight: TerminalColor;
      };
      removed: {
        background: TerminalColor;
        bar: TerminalColor;
        highlight: TerminalColor;
      };
      unchanged: {
        bar: TerminalColor;
      };
    };
  };
  textStyles: {
    label: TextStyle;
    selectedLabel: TextStyle;
  };
}
