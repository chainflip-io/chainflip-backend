#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { ExecutableTest } from '../shared/executable_test';
import { testSwapViaContract } from '../shared/swapping';
import { observeEvent, observeBadEvent } from '../shared/utils/substrate';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testRotateAndSwap = new ExecutableTest('Rotation-Barrier', main, 1200); // TODO JAMIE: unknown timeout

// Testing broadcast through vault rotations
async function main() {
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Wait for the activation key to be created and the activation key to be sent for signing
  testRotateAndSwap.log(`Vault rotation initiated`);
  await observeEvent('evmThresholdSigner:KeygenSuccess').event;
  testRotateAndSwap.log(`Waiting for the bitcoin key handover`);
  await observeEvent('bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  testRotateAndSwap.log(`Waiting for eth key activation transaction to be sent for signing`);
  await observeEvent('evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvent(':BroadcastAborted', { label: 'Rotate and swap' });

  // Using Arbitrum as the ingress chain to make the swap as fast as possible
  await testSwapViaContract('ArbEth', 'Eth');

  await broadcastAborted.stop();
}
