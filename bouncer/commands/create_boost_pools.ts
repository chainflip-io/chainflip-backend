#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument:
// A list of BoostPoolId objects, each containing an asset and a tier.
//
// Submits a single governance extrinsic that creates the boost pools for the given assets and tiers.
// All assets must be be from the same chain.
// For example: ./commands/create_boost_pools.ts '[{"asset": "Eth","tier": 5}, {"asset": "Eth","tier": 10}, {"asset": "Eth","tier": 30}]'

import { runWithTimeoutAndExit } from '../shared/utils';
import { BoostPoolId, createBoostPools } from '../shared/setup_boost_pools';

const newPools: BoostPoolId[] = JSON.parse(process.argv[2]);
await runWithTimeoutAndExit(createBoostPools(newPools), 30);
