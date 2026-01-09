import { connectContainerToNetwork, disconnectContainerFromNetwork } from 'shared/docker_utils';
import { sleep } from 'shared/utils';
import { testSwap } from 'shared/swapping';
import { TestContext } from 'shared/utils/test_context';
import { newChainflipIO } from 'shared/utils/chainflip_io';

// Testing a swap after temporarily disconnecting external nodes
export async function testSwapAfterDisconnection(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
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

  await cf.all([
    (subcf) => testSwap(subcf, 'Btc', 'Flip', undefined, undefined, testContext.swapContext),
    (subcf) => testSwap(subcf, 'Eth', 'Usdc', undefined, undefined, testContext.swapContext),
    (subcf) => testSwap(subcf, 'HubDot', 'Btc', undefined, undefined, testContext.swapContext),
  ]);
}
