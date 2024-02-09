module.exports = {
  dependencyTypes: ['dev', 'prod', 'resolutions', 'overrides'],
  filter: '.',
  indent: '  ',
  semverGroups: [],
  semverRange: '',
  versionGroups: [
    {
      dependencies: ['**'],
      packages: ['**'],
    },
  ],
};
