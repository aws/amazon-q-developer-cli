/** Shared color tokens for the sample app. */
export const colors = {
  bg: '#2d2a2e',
  fg: '#fcfcfa',
  accent: '#ffd866',
  danger: '#ff6188',
} as const;

export type ColorToken = keyof typeof colors;
