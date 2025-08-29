import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { TestContext } from 'shared/utils/test_context';
import { observeEvent, observeBadEvent, getChainflipApi } from 'shared/utils/substrate';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { InternalAssets } from '@chainflip/cli';
import {
  amountToFineAmount,
  assetDecimals,
  createStateChainKeypair,
  lpMutex,
  newAssetAddress,
  observeBalanceIncrease,
  waitForExt,
} from 'shared/utils';

// Testing broadcast through vault rotations
export async function testRotationBarrier(testContext: TestContext) {
  const { logger } = testContext;

  const lpUri = process.env.LP_URI || '//LP_1';
  const withdrawalAddress = await newAssetAddress(InternalAssets.ArbEth);

  await depositLiquidity(logger, InternalAssets.ArbEth, 5, false, lpUri);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  // Wait for the activation key to be created and the activation key to be sent for signing
  logger.info(`Vault rotation initiated`);
  await observeEvent(logger, 'evmThresholdSigner:KeygenSuccess').event;
  logger.info(`Waiting for the bitcoin key handover`);
  await observeEvent(logger, 'bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  logger.info(`Waiting for EVM key activation transaction to be sent for signing`);
  await observeEvent(logger, 'evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvent(logger, ':BroadcastAborted', {});

  logger.info(`Submitting withdrawal request.`);
  const api = await getChainflipApi();
  const { promise, waiter } = waitForExt(api, logger, 'InBlock', await lpMutex.acquire(lpUri));
  const lp = createStateChainKeypair(lpUri);
  const nonce = Number(await api.rpc.system.accountNextIndex(lp.address));
  const unsub = await api.tx.liquidityProvider
    .withdrawAsset(
      amountToFineAmount('2', assetDecimals(InternalAssets.ArbEth)),
      InternalAssets.ArbEth,
      {
        Arb: withdrawalAddress,
      },
    )
    .signAndSend(lp, { nonce }, waiter);

  const events = await promise;
  unsub();

  const egressId = events
    .find(({ event }) => event.method.endsWith('WithdrawalEgressScheduled'))
    ?.event.data[0].toHuman();

  logger.info(
    `Withdrawal extrinsic included in a block, scheduled egress ID ${JSON.stringify(egressId)}`,
  );

  const event = await observeEvent(logger, 'arbitrumIngressEgress:BatchBroadcastRequested', {
    test: (e) => JSON.stringify(e.data.egressIds).includes(JSON.stringify(egressId)),
    historicalCheckBlocks: 10,
  }).event;

  const broadcastId = Number(event.data.broadcastId);

  await observeEvent(logger, 'arbitrumBroadcaster:TransactionBroadcastRequest', {
    test: (e) => Number(e.data.broadcastId) === broadcastId,
    historicalCheckBlocks: 10,
  }).event;

  logger.info(`Broadcast requested for egress ID ${egressId}. Waiting for balance increase...`);

  await observeBalanceIncrease(logger, InternalAssets.ArbEth, withdrawalAddress);

  await broadcastAborted.stop();
}
