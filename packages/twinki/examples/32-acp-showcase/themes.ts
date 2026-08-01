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
  "kiro-dark|Kiro Dark|#18171c|#211f26|#2d2933|#f5f2f7|#b2abb9|#49434f|#b99cff|#75d6a2|#e9c46a|#f18491|monokai",
  "graphite|Graphite|#15171a|#1d2024|#282c32|#f2f3f5|#aeb4bd|#444a53|#63c7c2|#7fd39b|#e4bd68|#ef7c87|vitesse-dark",
  "paper|Paper|#f7f7f4|#efefeb|#e2e4df|#24272a|#62696e|#a6aca9|#166b8f|#2f7d4a|#8a5d00|#b33b47|github-light|#ffffff",
  "contrast|High Contrast|#000000|#151515|#2a2a2a|#ffffff|#d0d0d0|#666666|#d8c4ff|#7ff0ad|#ffe07a|#ff8794|github-dark-high-contrast",
  "monokai|Monokai|#27252a|#302e33|#3a373e|#fcfcfa|#b9b4bc|#504b54|#b9a5f5|#a9dc76|#ffd866|#ff6b91|monokai",
  "dracula|Dracula|#282a36|#30323f|#3a3d4c|#f8f8f2|#b5bad7|#53576b|#bd93f9|#50e88a|#e8e58c|#ff6e78|dracula",
  "github-dark|GitHub Dark|#0d1117|#161b22|#21262d|#f0f6fc|#aab4c0|#484f58|#d2a8ff|#7ee787|#d9b65f|#ff7b72|github-dark",
  "catppuccin|Catppuccin|#1e1e2e|#272738|#313244|#cdd6f4|#b3bad4|#4f5268|#cba6f7|#a6e3a1|#f9e2af|#f38ba8|catppuccin-mocha",
  "nord|Nord|#2e3440|#353c49|#404856|#eceff4|#bdc6d3|#4c566a|#bd96b9|#a3be8c|#ebcb8b|#df818a|nord",
  "one-dark|One Dark Pro|#282c34|#30353e|#3a404b|#e6e9ef|#b4bac5|#5c6370|#c678dd|#98c379|#e5c07b|#e5747c|one-dark-pro",
  "tokyo-night|Tokyo Night|#1a1b26|#222431|#2b2e3f|#c0caf5|#aab4d8|#565f89|#bb9af7|#9ece6a|#e0af68|#f7768e|tokyo-night",
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
