#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools, zero to infinity range orders and boost pools for all currencies.
// For example: ./commands/setup_concurrent.ts
// Setup_vaults.ts must be ran first.
import { existsSync, unlinkSync } from 'fs';
import { setupBoostPools } from 'shared/setup_boost_pools';
import { setupElections } from 'shared/setup_elections';
import { setupLendingPools } from 'shared/lending';
import { setupSwaps } from 'shared/setup_swaps';
import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { stateChainEventLogFile } from 'shared/utils/substrate';
import { newChainflipIO } from 'shared/utils/chainflip_io';

async function main(): Promise<void> {
  const cf = await newChainflipIO(globalLogger, []);
  cf.info('Setup concurrent');

  // Remove the old state chain events log file if it exists
  if (stateChainEventLogFile && existsSync(stateChainEventLogFile)) {
    unlinkSync(stateChainEventLogFile);
  }

  await cf.all([
    (subcf) => setupSwaps(subcf),
    (subcf) => setupBoostPools(subcf.logger),
    (subcf) => setupLendingPools(subcf),
    (subcf) => setupElections(subcf.logger),
  ]);
  cf.info('Setup concurrent complete');
}

await runWithTimeoutAndExit(main(), 240);
