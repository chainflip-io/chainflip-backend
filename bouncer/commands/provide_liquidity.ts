#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund liquidity of the given currency and amount
// For example: ./commands/provide_liquidity.ts btc 1.5

import { Asset } from '@chainflip-io/cli';
import { runWithTimeout } from '../shared/utils';
import { provideLiquidity } from '../shared/provide_liquidity';

async function main() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const amount = parseFloat(process.argv[3]);
  await provideLiquidity(ccy, amount);
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
