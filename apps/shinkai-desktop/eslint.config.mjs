import baseConfig from '../../eslint.config.mjs';

export default [
  {
    ignores: ['**/dist', '**/src-tauri/target/**'],
  },
  ...baseConfig,
  {
    files: [
      '**/*.ts',
      '**/*.cts',
      '**/*.mts',
      '**/*.tsx',
      '**/*.js',
      '**/*.cjs',
      '**/*.mjs',
      '**/*.jsx',
    ],
    // Override or add rules here
    rules: {},
  },
  {
    files: ['**/*.ts', '**/*.cts', '**/*.mts', '**/*.tsx'],
    // Override or add rules here
    rules: {},
  },
  {
    files: ['**/*.js', '**/*.cjs', '**/*.mjs', '**/*.jsx'],
    // Override or add rules here
    rules: {},
  },
  {
    files: ['**/*.json'],
    rules: {
      '@nx/dependency-checks': [
        'error',
        {
          ignoredFiles: ['{projectRoot}/vite.config.{js,ts,mjs,mts}'],
        },
      ],
    },
    languageOptions: {
      parser: await import('jsonc-eslint-parser'),
    },
  },
];
