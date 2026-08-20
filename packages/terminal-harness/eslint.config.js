import tseslint from 'typescript-eslint';
import sonarjs from 'eslint-plugin-sonarjs';

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      '**/build/**',
      '**/coverage/**',
      '**/generated/**',
      '**/playwright-report/**',
      '**/test-results/**',
    ],
  },
  {
    // A suppression that no longer suppresses anything must not keep counting
    // against the debt baseline.
    linterOptions: { reportUnusedDisableDirectives: 'error' },
  },
  {
    files: ['**/*.{js,jsx,ts,tsx}'],
    languageOptions: {
      parser: tseslint.parser,
      ecmaVersion: 'latest',
      sourceType: 'module',
    },
    plugins: {
      sonarjs,
    },
    rules: {
      'complexity': ['error', 30],
      'max-depth': ['error', 5],
      'max-params': ['error', 6],
      'sonarjs/cognitive-complexity': ['error', 30],
    },
  },
);
