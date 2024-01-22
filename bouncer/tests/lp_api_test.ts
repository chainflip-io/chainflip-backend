#!/usr/bin/env -S pnpm tsx
import { testLpApi } from '../shared/lp_api_test';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await testLpApi();
  process.exit(0);
}

runWithTimeout(main(), 300000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
