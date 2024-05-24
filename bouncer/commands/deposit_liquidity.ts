#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command 3 two arguments.
// 1 - Asset
// 2 - Amount
// 3 (optional) - Account URI (Default: "//LP_1")
// It will fund liquidity of the given currency and amount
// For example: ./commands/deposit_liquidity.ts Btc 1.5 '//LP_2'

import { parseAssetString, executeWithTimeout } from '../shared/utils';
import { depositLiquidity } from '../shared/deposit_liquidity';

await executeWithTimeout(
  depositLiquidity(
    parseAssetString(process.argv[2]),
    parseFloat(process.argv[3]),
    false,
    process.argv[4],
  ),
  120,
);
