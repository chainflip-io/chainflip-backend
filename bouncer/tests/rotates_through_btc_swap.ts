import { requestNewSwap, doPerformSwap } from 'shared/perform_swap';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { observeEvent } from 'shared/utils/substrate';
import { prepareSwap, testSwap } from 'shared/swapping';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

async function rotatesThroughBtcSwap<A = []>(cf: ChainflipIO<A>, testContext: TestContext) {
  const sourceAsset = 'Btc';
  const destAsset = 'ArbEth';

  const { destAddress, tag } = await prepareSwap(
    cf.logger,
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'through rotation',
    testContext.swapContext,
  );
  const subCf = cf.withChildLogger(tag);

  subCf.debug('Generated ArbEth address: ' + destAddress);

  const swapParams = await requestNewSwap(subCf, sourceAsset, destAsset, destAddress);

  const newEpochEvent = observeEvent(subCf.logger, 'validator:NewEpoch').event;
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  subCf.info(`Vault rotation initiated. Awaiting new epoch.`);
  await newEpochEvent;
  subCf.info('Vault rotated!');

  await doPerformSwap(subCf, swapParams, undefined, undefined, undefined, testContext.swapContext);
}

export async function testRotatesThroughBtcSwap(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await rotatesThroughBtcSwap(cf, testContext);
  await testSwap(
    cf,
    'ArbEth',
    'Btc',
    undefined,
    undefined,
    testContext.swapContext,
    'after rotation',
  );
}
