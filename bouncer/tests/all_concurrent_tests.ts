#!/usr/bin/env -S pnpm tsx
import { testLpDepositExpiry } from '../shared/lp_deposit_expiry';
import { testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

async function runAllConcurrentTests() {
  await Promise.all([testAllSwaps(), testLpDepositExpiry()]);
}

runWithTimeout(runAllConcurrentTests(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
