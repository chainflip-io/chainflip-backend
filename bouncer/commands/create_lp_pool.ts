#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a new liquidity pool for the given currency and
// initial price in USDC
// For example: ./commands/create_lp_pool.ts btc 10000

import { createLpPool } from '../shared/create_lp_pool';
import { parseAssetString, runWithTimeout } from '../shared/utils';

async function main() {
  const initialPrice = parseFloat(process.argv[3]);
  await createLpPool(parseAssetString(process.argv[2]), initialPrice);
  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
