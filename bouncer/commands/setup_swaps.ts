#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { setupSwaps } from '../shared/setup_swaps';
import { runWithTimeout } from '../shared/utils';

runWithTimeout(setupSwaps(), 240000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
