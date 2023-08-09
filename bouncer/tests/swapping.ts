#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await testAllSwaps();
  process.exit(0);
}

runWithTimeout(main(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
