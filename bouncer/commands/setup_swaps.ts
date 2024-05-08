#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { setupSwaps } from '../shared/setup_swaps';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await setupSwaps();
  process.exit(0);
}

runWithTimeout(main(), 240000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
