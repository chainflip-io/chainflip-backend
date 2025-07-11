#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will observe the chainflip state-chain until the block with the blocknumber given by the argument
// is observed

// For example: ./commands/observe_block.ts 3
// will wait until block number 3 has appeared on the state chain

import { runWithTimeout, sleep } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';

async function main(): Promise<void> {
  await using api = await getChainflipApi();
  const expectedBlock = process.argv[2];
  while ((await api.rpc.chain.getBlockHash(expectedBlock)).every((e) => e === 0)) {
    await sleep(1000);
  }
  process.exit(0);
}

runWithTimeout(main(), 60).catch(() => {
  console.log('Failed to observe block no. ' + process.argv[2]);
  process.exit(-1);
});
