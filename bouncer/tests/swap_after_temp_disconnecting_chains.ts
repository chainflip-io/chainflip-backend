#!/usr/bin/env -S pnpm tsx
import { connectContainerToNetwork, disconnectContainerFromNetwork } from '../shared/docker_utils';
import { sleep } from '../shared/utils';
import { testSwap } from '../shared/swapping';

try {
  console.log('=== Testing a swap after temporarily disconnecting external nodes ===');

  const networkName = 'chainflip-localnet_default';
  const allExternalNodes = ['bitcoin', 'geth', 'polkadot'];

  allExternalNodes.forEach((container) => {
    disconnectContainerFromNetwork(container, networkName);
  });

  await sleep(10000);

  allExternalNodes.forEach((container) => {
    connectContainerToNetwork(container, networkName);
  });

  await testSwap('DOT', 'BTC');

  console.log('=== Test complete ===');

  process.exit(0);
} catch (e) {
  console.log('Error: ', e);
  process.exit(1);
}
