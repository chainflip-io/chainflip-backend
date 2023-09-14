#!/usr/bin/env -S pnpm tsx
import { testFundRedeem } from '../shared/fund_redeem';
import { runWithTimeout } from '../shared/utils';

runWithTimeout(testFundRedeem('redeem'), 600000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
