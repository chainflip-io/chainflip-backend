#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools, zero to infinity range orders and boost pools for all currencies.
// For example: ./commands/setup_concurrent.ts
// Setup_vaults.ts must be ran first.
import { setupBoostPools } from '../shared/setup_boost_pools';
import { setupSwaps } from '../shared/setup_swaps';
import { runWithTimeoutAndExit } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

async function main(): Promise<void> {
  globalLogger.info('=== Setup concurrent ===');
  await Promise.all([setupSwaps(globalLogger), setupBoostPools(globalLogger)]);
  globalLogger.info('=== Setup concurrent complete ===');
}

await runWithTimeoutAndExit(main(), 240);
