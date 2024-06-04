#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument:
// A list of BoostPoolId objects, each containing an asset and a tier.
//
// Submits a single governance extrinsic that creates the boost pools for the given assets and tiers.
// All assets must be be from the same chain.
// For example: ./commands/create_boost_pools.ts '[{"asset": "Eth","tier": 5}, {"asset": "Eth","tier": 10}, {"asset": "Eth","tier": 30}]'

import { runWithTimeout } from '../shared/utils';
import { createBoostPools, BoostPoolId } from '../shared/setup_boost_pools';

async function main(): Promise<void> {
  const newPools: BoostPoolId[] = JSON.parse(process.argv[2]);
  await createBoostPools(newPools);
  process.exit(0);
}

runWithTimeout(main(), 30000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
