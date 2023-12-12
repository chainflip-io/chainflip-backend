#!/usr/bin/env -S pnpm tsx
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  await testPolkadotRuntimeUpdate();
  process.exit(0);
}

runWithTimeout(main(), 1230000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
