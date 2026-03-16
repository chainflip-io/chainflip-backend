import { connectContainerToNetwork, disconnectContainerFromNetwork } from 'shared/docker_utils';
import { sleep } from 'shared/utils';
import { testSwap } from 'shared/swapping';
import { TestContext } from 'shared/utils/test_context';
import { newChainflipIO } from 'shared/utils/chainflip_io';

// Testing a swap after temporarily disconnecting external nodes
export async function testSwapAfterDisconnection(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
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

  await cf.all([
    (subcf) => testSwap(subcf, 'Btc', 'Flip', undefined, undefined, testContext.swapContext),
    (subcf) => testSwap(subcf, 'Eth', 'Usdc', undefined, undefined, testContext.swapContext),
  ]);
}
