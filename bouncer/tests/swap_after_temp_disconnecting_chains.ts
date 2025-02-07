#!/usr/bin/env -S pnpm tsx
import { connectContainerToNetwork, disconnectContainerFromNetwork } from '../shared/docker_utils';
import { sleep } from '../shared/utils';
import { testSwap } from '../shared/swapping';
import { ExecutableTest } from '../shared/executable_test';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testSwapAfterDisconnection = new ExecutableTest(
  'Swap-After-Disconnection',
  main,
  1300,
);

// Testing a swap after temporarily disconnecting external nodes
async function main() {
  const networkName = 'chainflip-localnet_default';
  const allExternalNodes = ['bitcoin', 'geth'];

  await Promise.all(
    allExternalNodes.map((container) => disconnectContainerFromNetwork(container, networkName)),
  );

  await sleep(10000);

  await Promise.all(
    allExternalNodes.map((container) => connectContainerToNetwork(container, networkName)),
  );

  await Promise.all([
    testSwap('Btc', 'Flip', undefined, undefined, testSwapAfterDisconnection.swapContext),
    testSwap('Eth', 'Usdc', undefined, undefined, testSwapAfterDisconnection.swapContext),
  ]);
}
