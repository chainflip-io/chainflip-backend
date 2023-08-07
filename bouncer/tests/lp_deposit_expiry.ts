#!/usr/bin/env -S pnpm tsx
import { testLpDepositExpiry } from '../shared/lp_deposit_expiry';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await testLpDepositExpiry();
  console.log('=== LP deposit expiry test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
