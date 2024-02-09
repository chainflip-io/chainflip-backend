import { ESLint } from 'eslint';

export default {
  '*.{ts,js}': [
    async (files) => {
      const eslint = new ESLint();
      const isIgnored = await Promise.all(
        files.map((file) => eslint.isPathIgnored(file)),
      );
      const filteredFiles = files.filter((_, i) => !isIgnored[i]);
      return [`eslint --max-warnings=0 ${filteredFiles.join(' ')}`];
    },
    'prettier --check',
  ],
  '*.{yaml,yml}': ['prettier --check'],
};
