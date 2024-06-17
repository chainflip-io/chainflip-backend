#!/usr/bin/env -S pnpm tsx
import { requestNewSwap, performSwap, doPerformSwap } from '../shared/perform_swap';
import { newAddress } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { observeEvent } from '../shared/utils/substrate';

async function rotatesThroughBtcSwap() {
  const tag = `Btc -> Dot (through rotation)`;
  const address = await newAddress('Dot', 'foo');

  console.log('Generated Dot address: ' + address);

  const swapParams = await requestNewSwap('Btc', 'Dot', address, tag);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  console.log(`Vault rotation initiated. Awaiting new epoch.`);
  await observeEvent('validator:NewEpoch').event;
  console.log('Vault rotated!');

  await doPerformSwap(swapParams, tag);
}

async function swapAfterRotation() {
  const sourceAsset = 'Dot';
  const destAsset = 'Btc';

  const address = await newAddress(destAsset, 'bar');
  const tag = `${sourceAsset} -> ${destAsset} (after rotation)`;

  await performSwap(sourceAsset, destAsset, address, tag);
}

try {
  console.log('=== Testing BTC swaps through vault rotations ===');
  await rotatesThroughBtcSwap();
  await swapAfterRotation();
  console.log('=== Test complete ===');
  process.exit(0);
} catch (e) {
  console.error(e);
  process.exit(-1);
}
