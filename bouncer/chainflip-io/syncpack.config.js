module.exports = {
  dependencyTypes: ['dev', 'peer', 'prod', 'resolutions', 'overrides'],
  filter: '.',
  indent: '  ',
  overrides: false,
  semverGroups: [],
  semverRange: '',
  versionGroups: [
    {
      dependencies: ['**'],
      packages: ['**'],
    },
  ],
};
