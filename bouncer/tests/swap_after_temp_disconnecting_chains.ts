import { connectContainerToNetwork, disconnectContainerFromNetwork } from '../shared/docker_utils';
import { sleep } from '../shared/utils';
import { testSwap } from '../shared/swapping';
import { TestContext } from '../shared/utils/test_context';

// Testing a swap after temporarily disconnecting external nodes
export async function testSwapAfterDisconnection(testContext: TestContext) {
  const networkName = 'chainflip-localnet_default';
  const allExternalNodes = ['bitcoin', 'geth'];

  await Promise.all(
    allExternalNodes.map((container) =>
      disconnectContainerFromNetwork(testContext.logger, container, networkName),
    ),
  );

  await sleep(10000);

  await Promise.all(
    allExternalNodes.map((container) =>
      connectContainerToNetwork(testContext.logger, container, networkName),
    ),
  );

  await Promise.all([
    testSwap(testContext.logger, 'Btc', 'Flip', undefined, undefined, testContext.swapContext),
    testSwap(testContext.logger, 'Eth', 'Usdc', undefined, undefined, testContext.swapContext),
  ]);
}
