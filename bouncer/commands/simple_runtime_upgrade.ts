#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Performs a noop runtime upgrade. You will bump the runtime `spec_version` and nothing else.
// This should not affect the CFEs in any way. Everything should just function straight through the upgrade.
//
// Optional args:
// -test: Run the swap tests after the upgrade.
//
// For example ./commands/simple_runtime_upgrade.ts
// NB: It *must* be run from the bouncer directory.

import path from 'path';
import { simpleRuntimeUpgrade } from '../shared/simple_runtime_upgrade';
import { executeWithTimeout } from '../shared/utils';
import { testAllSwaps } from '../tests/all_swaps';

async function main(): Promise<void> {
  await simpleRuntimeUpgrade(path.dirname(process.cwd()));

  if (process.argv[2] === '-test') {
    await testAllSwaps.run();
  }
}

// 15 minute timeout. We need to wait for user input, compile, and potentially run tests. This is deliberately quite long.
// This won't be run on CI, so it's not a problem if it takes a while.
await executeWithTimeout(main(), 15 * 60);
