/* eslint-disable import/no-extraneous-dependencies */
import { defineConfig } from 'tsup';

export default defineConfig({
  treeshake: true,
  minify: false,
  dts: true,
  skipNodeModulesBundle: true,
  format: ['cjs', 'esm'],
  entry: {
    lib: 'src/lib/index.ts',
    cli: 'src/main.ts',
  },
  sourcemap: true,
  target: 'es2022',
});
