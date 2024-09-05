import { requestNewSwap, performSwap, doPerformSwap } from '../shared/perform_swap';
import { newAddress } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from '../shared/utils/substrate';
import { ExecutableTest } from '../shared/executable_test';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testRotatesThroughBtcSwap = new ExecutableTest('Rotates-Through-BTC-Swap', main, 1200); // TODO JAMIE: unknown timeout

async function rotatesThroughBtcSwap() {
  const tag = `Btc -> Dot (through rotation)`;
  const address = await newAddress('Dot', 'foo');

  testRotatesThroughBtcSwap.log('Generated Dot address: ' + address);

  const swapParams = await requestNewSwap('Btc', 'Dot', address, tag);

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

async function swapAfterRotation() {
  const sourceAsset = 'Dot';
  const destAsset = 'Btc';

  const address = await newAddress(destAsset, 'bar');
  const tag = `${sourceAsset} -> ${destAsset} (after rotation)`;

  await performSwap(
    sourceAsset,
    destAsset,
    address,
    tag,
    undefined,
    undefined,
    undefined,
    undefined,
    true,
    testRotatesThroughBtcSwap.swapContext,
  );
}

async function main() {
  await rotatesThroughBtcSwap();
  await swapAfterRotation();
}
