// eslint-disable-next-line @typescript-eslint/no-var-requires
const path = require('path');

module.exports = {
  extends: '../../.eslintrc.json',
  plugins: ['eslint-plugin-n'],
  rules: {
    'n/no-process-env': ['error'],
    'no-await-in-loop': 'off',
    'import/no-extraneous-dependencies': [
      'error',
      {
        packageDir: [__dirname, path.join(__dirname, '..', '..')],
      },
    ],
    'no-restricted-imports': [
      'error',
      {
        patterns: [
          {
            group: ['graphql-request'],
            importNames: ['gql'],
            message:
              'Import "gql" from "src/gql/generated" instead of "graphql-request"',
          },
        ],
      },
    ],
  },
  overrides: [
    {
      files: ['*.test.ts', '*.mjs', '*.js', '*.cjs'],
      rules: {
        'n/no-process-env': ['off'],
      },
    },
  ],
};
