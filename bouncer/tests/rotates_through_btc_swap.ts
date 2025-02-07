import { requestNewSwap, doPerformSwap } from '../shared/perform_swap';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from '../shared/utils/substrate';
import { prepareSwap, testSwap } from '../shared/swapping';
import { TestContext } from '../shared/utils/test_context';

async function rotatesThroughBtcSwap(testContext: TestContext) {
  const logger = testContext.logger;
  const sourceAsset = 'Btc';
  const destAsset = 'Dot';

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'through rotation',
    true,
    testContext.swapContext,
  );

  logger.debug('Generated Dot address: ' + destAddress);

  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  logger.info(`Vault rotation initiated. Awaiting new epoch.`);
  await observeEvent('validator:NewEpoch').event;
  logger.info('Vault rotated!');

  await doPerformSwap(
    swapParams,
    tag,
    undefined,
    undefined,
    undefined,
    true,
    testContext.swapContext,
  );
}

export async function testRotatesThroughBtcSwap(testContext: TestContext) {
  await rotatesThroughBtcSwap(testContext);
  await testSwap('Dot', 'Btc', undefined, undefined, testContext.swapContext, 'after rotation');
}
