#!/usr/bin/env -S pnpm tsx
import { connectContainerToNetwork, disconnectContainerFromNetwork } from '../shared/docker_utils';
import { sleep } from '../shared/utils';
import { testSwap } from '../shared/swapping';

try {
  console.log('=== Testing a swap after temporarily disconnecting external nodes ===');

  const networkName = 'chainflip-localnet_default';
  const allExternalNodes = ['bitcoin', 'geth', 'polkadot'];

  await Promise.all(
    allExternalNodes.map((container) => disconnectContainerFromNetwork(container, networkName)),
  );

  await sleep(10000);

  await Promise.all(
    allExternalNodes.map((container) => connectContainerToNetwork(container, networkName)),
  );

  await Promise.all([testSwap('Dot', 'Btc'), testSwap('Btc', 'Flip'), testSwap('Eth', 'Usdc')]);

  console.log('=== Test complete ===');

  process.exit(0);
} catch (e) {
  console.log('Error: ', e);
  process.exit(1);
}
