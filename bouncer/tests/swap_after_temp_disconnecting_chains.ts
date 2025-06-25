import { connectContainerToNetwork, disconnectContainerFromNetwork } from 'shared/docker_utils';
import { sleep } from 'shared/utils';
import { testSwap } from 'shared/swapping';
import { TestContext } from 'shared/utils/test_context';

// Testing a swap after temporarily disconnecting external nodes
export async function testSwapAfterDisconnection(testContext: TestContext) {
  const networkName = 'chainflip-localnet_default';
  // We use assethub here to test that it applies to assethub, and Polkadot by proxy.
  // We don't test Polkadot directly, since shutting down the Polkadot node in this way causes sync issues between
  // Polkadot and AssetHub which can take some time to resolve. Given AssetHub shares most of the same code as Polkadot,
  // this is a good proxy.
  const allExternalNodes = ['bitcoin', 'geth', 'assethub'];

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
    testSwap(testContext.logger, 'HubDot', 'Btc', undefined, undefined, testContext.swapContext),
  ]);
}
