#!/usr/bin/env -S pnpm tsx
import { testFundRedeem } from '../shared/fund_redeem';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await testFundRedeem();
  process.exit(0);
}

runWithTimeout(main(), 800000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
