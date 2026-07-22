import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      '**/build/**',
      '**/generated/**',
      'e2e_tests/test_fixtures/**',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      'no-console': 'off',
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-empty-object-type': 'off',
      '@typescript-eslint/no-require-imports': 'off',
      'react-hooks/rules-of-hooks': 'warn',
      'react-hooks/exhaustive-deps': 'warn',
      'no-restricted-imports': ['error', {
        paths: [{
          name: 'supports-color',
          message: 'Use { supportsColor } from "chalk" instead to stay in sync with chalk\'s detection.',
        }, {
          name: 'chalk',
          message: 'Import the shared { chalk } instance from utils/color instead, so the color level stays consistent across modules and FORCE_COLOR is never mutated.',
        }],
        patterns: ['chalk/*'],
      }],
    },
  },
  {
    files: ['src/utils/color.ts'],
    rules: {
      'no-restricted-imports': ['error', {
        paths: [{
          name: 'supports-color',
          message: 'Use { supportsColor } from "chalk" instead to stay in sync with chalk\'s detection.',
        }],
        patterns: ['chalk/*'],
      }],
    },
  },
  {
    files: ['**/*.stories.tsx'],
    rules: {
      'react-hooks/rules-of-hooks': 'off',
    },
  },
);
