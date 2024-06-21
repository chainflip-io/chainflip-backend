#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { setupSwaps } from '../shared/setup_swaps';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(setupSwaps(), 240);
