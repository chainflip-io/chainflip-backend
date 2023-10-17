#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Performs a noop runtime upgrade. You will bump the runtime `spec_version` and nothing else.
// This should not affect the CFEs in any way. Everything should just function straight through the upgrade.
// For example ./commands/noop_runtime_upgrade.ts
// NB: It *must* be run from the bouncer directory.

import { noopRuntimeUpgrade } from '../shared/noop_runtime_upgrade';
import { promptUser } from '../shared/prompt_user';
import { testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await noopRuntimeUpgrade();

  await promptUser(
    'Would you like to test all swaps after the upgrade now? The vaults and liquidity must be set up already.',
  );

  await testAllSwaps();

  process.exit(0);
}

// 15 minutes. We need to wait for user input, compile, and potentially run tests. This is deliberatly quite long.
// This won't be run on CI, so it's not a problem if it takes a while.
runWithTimeout(main(), 15 * 60 * 1000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
