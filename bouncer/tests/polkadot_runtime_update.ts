#!/usr/bin/env -S pnpm tsx
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { getChainflipApi, observeEvent, runWithTimeout } from '../shared/utils';

let broadcastAbortedCount = 0;

async function observeBroadcastAborted(): Promise<void> {
  for (;;) {
    await observeEvent(':BroadcastAborted', await getChainflipApi());
    console.log('Broadcast aborted');
    broadcastAbortedCount++;
  }
}

async function main(): Promise<void> {
  observeBroadcastAborted();

  await testPolkadotRuntimeUpdate();

  console.log(`Broadcasts aborted: ${broadcastAbortedCount}`);

  process.exit(0);
}

runWithTimeout(main(), 1300000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
