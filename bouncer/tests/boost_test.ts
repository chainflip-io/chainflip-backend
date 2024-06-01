#!/usr/bin/env -S pnpm tsx
import { runWithTimeout } from '../shared/utils';
import { testBoostingSwap } from '../shared/boost';

async function main(): Promise<void> {
  await testBoostingSwap();
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
