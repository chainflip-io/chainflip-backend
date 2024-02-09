// eslint-disable-next-line import/no-relative-packages
import baseConfig from '../../jest.config.mjs';

export default {
  ...baseConfig,
  globalSetup: '<rootDir>/jest.setup.mjs',
  resetMocks: false,
  setupFiles: ['<rootDir>/src/__mocks__/env.mjs'],
};
