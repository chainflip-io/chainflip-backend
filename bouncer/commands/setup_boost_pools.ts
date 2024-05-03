#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// This command will create 3 tiers of boost pools for every asset. Tiers: 5, 10 and 30 bps.

import { setupBoostPools } from '../shared/setup_boost_pools';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await setupBoostPools();
  process.exit(0);
}

runWithTimeout(main(), 60000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
