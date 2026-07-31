export interface ShowcaseTheme {
  id: string;
  label: string;
  bg: string;
  panel: string;
  raised: string;
  fg: string;
  muted: string;
  border: string;
  accent: string;
  accentText: string;
  success: string;
  warning: string;
  danger: string;
  syntax: string;
}

const THEME_KEYS = [
  "id",
  "label",
  "bg",
  "panel",
  "raised",
  "fg",
  "muted",
  "border",
  "accent",
  "success",
  "warning",
  "danger",
  "syntax",
  "accentText",
] as const satisfies readonly (keyof ShowcaseTheme)[];

const THEME_ROWS = [
  "kiro-dark|Kiro Dark|#1a1a1e|#2a2a30|#342d3f|#f5f3f7|#aaa6b0|#56515d|#c19aff|#80ffb5|#ffd866|#ff8080|monokai",
  "graphite|Graphite|#17191c|#22252a|#2d3138|#f2f3f5|#969ca6|#4d535d|#6ed5d0|#80d49b|#f0c36a|#f27d88|vitesse-dark",
  "paper|Paper|#f7f7f4|#e7e9e5|#d9ddd8|#25282b|#6e7479|#a9afad|#166b8f|#2f7d4a|#9a6500|#b63d48|github-light|#ffffff",
  "contrast|High Contrast|#000000|#303030|#424242|#ffffff|#c4c4c4|#737373|#e2cfff|#92ffc2|#ffe783|#ff9da8|github-dark-high-contrast",
  "monokai|Monokai|#2d2a2e|#403e41|#49454b|#fcfcfa|#aaa7ab|#5b575d|#ab9df2|#a9dc76|#ffd866|#ff6188|monokai",
  "dracula|Dracula|#282a36|#44475a|#4d5065|#f8f8f2|#a6accd|#6272a4|#bd93f9|#50fa7b|#f1fa8c|#ff5555|dracula",
  "github-dark|GitHub Dark|#0d1117|#21262d|#2d333b|#f0f6fc|#9da7b3|#484f58|#d2a8ff|#7ee787|#e3b341|#ff7b72|github-dark",
  "catppuccin|Catppuccin|#1e1e2e|#313244|#3b3d52|#cdd6f4|#a6adc8|#585b70|#cba6f7|#a6e3a1|#f9e2af|#f38ba8|catppuccin-mocha",
  "nord|Nord|#2e3440|#3b4252|#434c5e|#eceff4|#aeb8c8|#4c566a|#b48ead|#a3be8c|#ebcb8b|#bf616a|nord",
  "one-dark|One Dark Pro|#282c34|#3e4451|#454c59|#e6e9ef|#a7adba|#5c6370|#c678dd|#98c379|#e5c07b|#e06c75|one-dark-pro",
  "tokyo-night|Tokyo Night|#1a1b26|#292e42|#343b58|#c0caf5|#9aa5ce|#565f89|#bb9af7|#9ece6a|#e0af68|#f7768e|tokyo-night",
] as const;

function parseTheme(row: string): ShowcaseTheme {
  const values = row.split("|");
  const theme = Object.fromEntries(THEME_KEYS.map((key, index) => [key, values[index]])) as unknown as ShowcaseTheme;
  return { ...theme, accentText: theme.accentText ?? theme.bg };
}

export const THEMES: readonly ShowcaseTheme[] = THEME_ROWS.map(parseTheme);

export function themeById(id?: string): ShowcaseTheme {
  return THEMES.find((theme) => theme.id === id) ?? THEMES[0]!;
}

export function nextTheme(current: ShowcaseTheme): ShowcaseTheme {
  const index = THEMES.findIndex((theme) => theme.id === current.id);
  return THEMES[(index + 1) % THEMES.length]!;
}
