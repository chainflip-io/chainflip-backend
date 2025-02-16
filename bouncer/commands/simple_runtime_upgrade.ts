#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Performs a noop runtime upgrade. You will bump the runtime `spec_version` and nothing else.
// This should not affect the CFEs in any way. Everything should just function straight through the upgrade.
//
// For example ./commands/simple_runtime_upgrade.ts
// NB: It *must* be run from the bouncer directory.

import path from 'path';
import { simpleRuntimeUpgrade } from '../shared/simple_runtime_upgrade';
import { runWithTimeoutAndExit } from '../shared/utils';

async function main(): Promise<void> {
  await simpleRuntimeUpgrade(path.dirname(process.cwd()));
}

// 15 minute timeout. We need to wait for user input, compile, and potentially run tests. This is deliberately quite long.
// This won't be run on CI, so it's not a problem if it takes a while.
await runWithTimeoutAndExit(main(), 15 * 60);
