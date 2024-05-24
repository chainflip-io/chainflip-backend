#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command 3 two arguments.
// 1 - Asset
// 2 - Amount
// 3 (optional) - Account URI (Default: "//LP_1")
// It will fund liquidity of the given currency and amount
// For example: ./commands/deposit_liquidity.ts Btc 1.5 '//LP_2'

import { parseAssetString, runWithTimeout } from '../shared/utils';
import { depositLiquidity } from '../shared/deposit_liquidity';

async function main() {
  const ccy = parseAssetString(process.argv[2]);
  const amount = parseFloat(process.argv[3]);
  await depositLiquidity(ccy, amount, false, process.argv[4]);
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
