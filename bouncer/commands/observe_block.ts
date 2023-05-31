#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes one argument.
// It will observe the chainflip state-chain until the block with the blocknumber given by the argument
// is observed

// For example: ./commands/observe_block.ts 3
// will wait until block number 3 has appeared on the state chain

import { ApiPromise, WsProvider } from '@polkadot/api';
import { runWithTimeout, sleep } from '../shared/utils';

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const expectedBlock = process.argv[2];
  const api = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
  while ((await api.rpc.chain.getBlockHash(expectedBlock)).every((e) => e === 0)) {
    await sleep(1000);
  }
  console.log('Observed block no. ' + expectedBlock);
  process.exit(0);
}

runWithTimeout(main(), 10000).catch(() => {
  console.log('Failed to observe block no. ' + process.argv[2]);
  process.exit(-1);
});
