#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Performs a noop runtime upgrade. You will bump the runtime `spec_version` and nothing else.
// This should not affect the CFEs in any way. Everything should just function straight through the upgrade.
//
// Optional args:
// -test: Run the swap tests after the upgrade.
// For example ./commands/noop_runtime_upgrade.ts
// NB: It *must* be run from the bouncer directory.

import { noopRuntimeUpgrade } from '../shared/noop_runtime_upgrade';
import { testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await noopRuntimeUpgrade();

  if (process.argv[2] === '-test') {
    await testAllSwaps();
  }

  process.exit(0);
}

// 15 minutes. We need to wait for user input, compile, and potentially run tests. This is deliberatly quite long.
// This won't be run on CI, so it's not a problem if it takes a while.
runWithTimeout(main(), 15 * 60 * 1000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
