import { highlight, listLanguages } from 'cli-highlight';
import { useMemo } from 'react';
import { useTheme } from '../hooks/useThemeContext.js';
import { resolveHighlightLanguage } from './highlight-languages.js';

const supportedLanguages = new Set(listLanguages());

// Hook for syntax highlighting using TUI theme colors
export function useSyntaxHighlight() {
  const { getColor } = useTheme();

  const syntaxTheme = useMemo(
    () => ({
      keyword: getColor('syntax.keyword'),
      built_in: getColor('syntax.built_in'),
      string: getColor('syntax.string'),
      comment: getColor('syntax.comment'),
      number: getColor('syntax.number'),
      literal: getColor('syntax.literal'),
      regexp: getColor('syntax.regexp'),
      function: getColor('syntax.function'),
      class: getColor('syntax.class'),
      type: getColor('syntax.type'),
      title: getColor('syntax.title'),
      name: getColor('syntax.name'),
      params: getColor('syntax.params'),
      variable: getColor('syntax.variable'),
      attr: getColor('syntax.attr'),
    }),
    [getColor]
  );

  const highlightCode = useMemo(
    () =>
      (code: string, language?: string): string => {
        const lang = resolveHighlightLanguage(language);
        // Skip unsupported languages to avoid cli-highlight errors
        if (lang && !supportedLanguages.has(lang)) {
          return code;
        }
        try {
          const noop = () => {};
          const origError = console.error;
          console.error = noop;
          try {
            return highlight(code, { language: lang, theme: syntaxTheme });
          } finally {
            console.error = origError;
          }
        } catch {
          return code;
        }
      },
    [syntaxTheme]
  );

  return highlightCode;
}
