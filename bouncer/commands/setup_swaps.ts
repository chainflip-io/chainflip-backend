#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will setup pools and zero to infinity range orders for all currencies
// For example: ./commands/setup_swaps.ts

import { setupSwaps } from 'shared/setup_swaps';
import { runWithTimeoutAndExit } from 'shared/utils';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';

const cf = await newChainflipIO(globalLogger, []);
await runWithTimeoutAndExit(setupSwaps(cf), 240);
