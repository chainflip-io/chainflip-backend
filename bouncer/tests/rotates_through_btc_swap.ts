import { requestNewSwap, doPerformSwap } from 'shared/perform_swap';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { observeEvent } from 'shared/utils/substrate';
import { prepareSwap, testSwap } from 'shared/swapping';
import { TestContext } from 'shared/utils/test_context';

async function rotatesThroughBtcSwap(testContext: TestContext) {
  const sourceAsset = 'Btc';
  const destAsset = 'ArbEth';

  const { destAddress, tag } = await prepareSwap(
    testContext.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'through rotation',
    testContext.swapContext,
  );
  const logger = testContext.logger.child({ tag });

  logger.debug('Generated ArbEth address: ' + destAddress);

  const swapParams = await requestNewSwap(logger, sourceAsset, destAsset, destAddress);

  const newEpochEvent = observeEvent(logger, 'validator:NewEpoch').event;
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  logger.info(`Vault rotation initiated. Awaiting new epoch.`);
  await newEpochEvent;
  logger.info('Vault rotated!');

  await doPerformSwap(logger, swapParams, undefined, undefined, undefined, testContext.swapContext);
}

export async function testRotatesThroughBtcSwap(testContext: TestContext) {
  await rotatesThroughBtcSwap(testContext);
  await testSwap(
    testContext.logger,
    'ArbEth',
    'Btc',
    undefined,
    undefined,
    testContext.swapContext,
    'after rotation',
  );
}
