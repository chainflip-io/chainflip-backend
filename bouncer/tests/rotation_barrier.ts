#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { testSwapViaContract } from '../shared/swapping';
import { observeEvent, observeBadEvents } from '../shared/utils/substrate';

async function rotateAndSwap() {
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Wait for the activation key to be created and the activation key to be sent for signing
  console.log(`Vault rotation initiated`);
  await observeEvent('evmThresholdSigner:KeygenSuccess').event;
  console.log(`Waiting for the bitcoin key handover`);
  await observeEvent('bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  console.log(`Waiting for eth key activation transaction to be sent for signing`);
  await observeEvent('evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvents(':BroadcastAborted', { label: 'Rotate and swap' });

  // Using Arbitrum as the ingress chain to make the swap as fast as possible
  await testSwapViaContract('ArbEth', 'Eth');

  await broadcastAborted.stop();
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
