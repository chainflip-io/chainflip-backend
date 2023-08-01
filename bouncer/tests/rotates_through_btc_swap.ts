#!/usr/bin/env -S pnpm tsx
import { requestNewSwap, performSwap, doPerformSwap } from '../shared/perform_swap';
import { newAddress, getChainflipApi, observeEvent } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function rotatesThroughBtcSwap() {
  const chainflip = await getChainflipApi();

  const tag = `BTC -> DOT (through rotation)`;
  const address = await newAddress('DOT', 'foo');

  console.log('Generated DOT address: ' + address);

  const swapParams = await requestNewSwap('BTC', 'DOT', address, 100, tag);

  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());
  console.log(`Vault rotation initiated. Awaiting new epoch.`);
  await observeEvent('validator:NewEpoch', chainflip);
  console.log('Vault rotated!');

  await doPerformSwap(swapParams, tag);
}

async function swapAfterRotation() {
  const sourceAsset = 'DOT';
  const destAsset = 'BTC';

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
