import { requestNewSwap, doPerformSwap } from '../shared/perform_swap';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from '../shared/utils/substrate';
import { ExecutableTest } from '../shared/executable_test';
import { prepareSwap, testSwap } from '../shared/swapping';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testRotatesThroughBtcSwap = new ExecutableTest('Rotates-Through-BTC-Swap', main, 360);

async function rotatesThroughBtcSwap() {
  const sourceAsset = 'Btc';
  const destAsset = 'Dot';

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    undefined,
    'through rotation',
    true,
    testRotatesThroughBtcSwap.swapContext,
  );

  testRotatesThroughBtcSwap.log('Generated Dot address: ' + destAddress);

  const swapParams = await requestNewSwap(sourceAsset, destAsset, destAddress, tag);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  testRotatesThroughBtcSwap.log(`Vault rotation initiated. Awaiting new epoch.`);
  await observeEvent('validator:NewEpoch').event;
  testRotatesThroughBtcSwap.log('Vault rotated!');

  await doPerformSwap(
    swapParams,
    tag,
    undefined,
    undefined,
    undefined,
    true,
    testRotatesThroughBtcSwap.swapContext,
  );
}

async function main() {
  await rotatesThroughBtcSwap();
  await testSwap(
    'Dot',
    'Btc',
    undefined,
    undefined,
    testRotatesThroughBtcSwap.swapContext,
    'after rotation',
  );
}
