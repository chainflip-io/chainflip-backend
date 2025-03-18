#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a new liquidity pool for the given currency and
// initial price in USDC
// For example: ./commands/create_lp_pool.ts btc 10000

import { createLpPool } from '../shared/create_lp_pool';
import { parseAssetString, runWithTimeoutAndExit } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

const initialPrice = parseFloat(process.argv[3]);
const asset = parseAssetString(process.argv[2]);
await runWithTimeoutAndExit(createLpPool(globalLogger, asset, initialPrice), 20);
