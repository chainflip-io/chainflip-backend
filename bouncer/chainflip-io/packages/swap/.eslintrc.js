module.exports = {
  extends: '../../.eslintrc.json',
  rules: {
    'no-await-in-loop': 'off',
    'import/no-extraneous-dependencies': [
      'error',
      {
        packageDir: [__dirname],
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
};
