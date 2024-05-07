#!/usr/bin/env -S pnpm tsx
import { getChainflipApi, observeBadEvents, observeEvent } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { testSwapViaContract } from '../shared/swapping';

async function rotateAndSwap() {
  await using chainflip = await getChainflipApi();

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Wait for the activation key to be created and the activation key to be sent for signing
  console.log(`Vault rotation initiated`);
  await observeEvent('evmThresholdSigner:KeygenSuccess', chainflip);
  console.log(`Waiting for the bitcoin key handover`);
  await observeEvent('bitcoinThresholdSigner:KeyHandoverSuccessReported', chainflip);
  console.log(`Waiting for eth key activation transaction to be sent for signing`);
  await observeEvent('evmThresholdSigner:ThresholdSignatureRequest', chainflip);

  let stopObserving = false;
  const broadcastAborted = observeBadEvents(':BroadcastAborted', () => stopObserving);

  // Using Arbitrum as the ingress chain to make the swap as fast as possible
  await testSwapViaContract('ArbEth', 'Eth');

  stopObserving = true;
  await broadcastAborted;
}

try {
  console.log('=== Testing broadcast through vault rotations ===');
  await rotateAndSwap();
  console.log('=== Test complete ===');
  process.exit(0);
} catch (e) {
  console.error(e);
  process.exit(-1);
}
