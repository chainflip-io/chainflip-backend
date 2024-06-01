#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools, zero to infinity range orders and boost pools for all currencies.
// For example: ./commands/setup_concurrent.ts
// Setup_vaults.ts must be ran first.
import { setupBoostPools } from '../shared/setup_boost_pools';
import { setupSwaps } from '../shared/setup_swaps';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  console.log('=== Setup concurrent ===');
  await Promise.all([setupSwaps(), setupBoostPools()]);
  console.log('=== Setup concurrent complete ===');
  process.exit(0);
}

runWithTimeout(main(), 240000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
