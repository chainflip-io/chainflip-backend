#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { TestContext } from '../shared/utils/test_context';
import { testVaultSwap } from '../shared/swapping';
import { observeEvent, observeBadEvent } from '../shared/utils/substrate';

// Testing broadcast through vault rotations
export async function testRotateAndSwap(testContext: TestContext) {
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Wait for the activation key to be created and the activation key to be sent for signing
  testContext.info(`Vault rotation initiated`);
  await observeEvent('evmThresholdSigner:KeygenSuccess').event;
  testContext.info(`Waiting for the bitcoin key handover`);
  await observeEvent('bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  testContext.info(`Waiting for eth key activation transaction to be sent for signing`);
  await observeEvent('evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvent(':BroadcastAborted', { label: 'Rotate and swap' });

  // Using Arbitrum as the ingress chain to make the swap as fast as possible
  await testVaultSwap('ArbEth', 'Eth');

  await broadcastAborted.stop();
}
