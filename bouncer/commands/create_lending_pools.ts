#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument:
// A list of LendingPoolId objects, each containing an asset.
//
// Submits a single governance extrinsic that creates the lending pools for the given assets.
// For example: ./commands/create_lending_pools.ts '[{"asset": "Btc"}, {"asset": "Eth"}, {"asset": "Sol"}, {"asset": "Usdc"}]'

import { createLendingPools, LendingPoolId } from 'shared/lending';
import { runWithTimeoutAndExit } from 'shared/utils';

import { globalLogger } from 'shared/utils/logger';

const newPools: LendingPoolId[] = JSON.parse(process.argv[2]);
await runWithTimeoutAndExit(createLendingPools(globalLogger, newPools), 30);
