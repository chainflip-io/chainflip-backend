#!/usr/bin/env -S pnpm tsx
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { runWithTimeout } from '../shared/utils';

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.
async function main(): Promise<void> {
  await testPolkadotRuntimeUpdate();
  process.exit(0);
}

runWithTimeout(main(), 1300000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
