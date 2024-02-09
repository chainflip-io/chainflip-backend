/* eslint-disable import/no-extraneous-dependencies */
import { defineConfig } from 'tsup';

export default defineConfig({
  treeshake: true,
  minify: false,
  dts: true,
  format: 'esm',
  entry: {
    lib: 'src/lib/index.ts',
    cli: 'src/main.ts',
  },
  sourcemap: true,
  target: 'es2022',
  banner: {
    js: `
import { fileURLToPath } from 'url';
import { createRequire as topLevelCreateRequire } from 'module';
import * as path from 'path';
const require = topLevelCreateRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
    `,
  },
});
