module.exports = {
  schema: 'https://indexer-perseverance.chainflip.io/graphql',
  documents: ['**/*.ts', '**/*.tsx'],
  extensions: {
    codegen: {
      generates: {
        'src/gql/generated/': {
          preset: 'client',
          presetConfig: {
            gqlTagName: 'gql',
            fragmentMasking: false,
          },
          config: {
            enumsAsTypes: true,
          },
        },
      },
    },
  },
};
