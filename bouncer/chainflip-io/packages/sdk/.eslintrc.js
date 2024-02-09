// eslint-disable-next-line @typescript-eslint/no-var-requires
const path = require('path');

module.exports = {
  extends: '../../.eslintrc.json',
  rules: {
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
            group: ['@/shared/node-apis/*'],
            message:
              'This directory uses Node.js APIs that are not browser compatible.',
          },
        ],
      },
    ],
  },
};
