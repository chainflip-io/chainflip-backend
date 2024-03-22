#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: ./commands/range_order.ts Btc 10

import { rangeOrder } from '../shared/range_order';
import { parseAssetString, runWithTimeout } from '../shared/utils';

async function main() {
  const ccy = parseAssetString(process.argv[2]);
  const amount = parseFloat(process.argv[3].trim());
  await rangeOrder(ccy, amount);
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
