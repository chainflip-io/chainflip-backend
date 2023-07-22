#!/usr/bin/env pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will observe the chainflip state-chain until the block with the blocknumber given by the argument
// is observed

// For example: ./commands/observe_block.ts 3
// will wait until block number 3 has appeared on the state chain

import { runWithTimeout, sleep, getChainflipApi } from '../shared/utils';

async function main(): Promise<void> {
  const api = await getChainflipApi();
  const expectedBlock = process.argv[2];
  while ((await api.rpc.chain.getBlockHash(expectedBlock)).every((e) => e === 0)) {
    await sleep(1000);
  }
  console.log('Observed block no. ' + expectedBlock);
  process.exit(0);
}

runWithTimeout(main(), 60000).catch(() => {
  console.log('Failed to observe block no. ' + process.argv[2]);
  process.exit(-1);
});
