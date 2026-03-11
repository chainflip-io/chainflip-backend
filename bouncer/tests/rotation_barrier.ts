import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { TestContext } from 'shared/utils/test_context';
import { observeEvent, observeBadEvent } from 'shared/utils/substrate';
import { depositLiquidity } from 'shared/deposit_liquidity';
import {
  amountToFineAmount,
  assetDecimals,
  Assets,
  newAssetAddress,
  observeBalanceIncrease,
} from 'shared/utils';
import { fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { liquidityProviderWithdrawalEgressScheduled } from 'generated/events/liquidityProvider/withdrawalEgressScheduled';
import { arbitrumIngressEgressBatchBroadcastRequested } from 'generated/events/arbitrumIngressEgress/batchBroadcastRequested';
import { arbitrumBroadcasterTransactionBroadcastRequest } from 'generated/events/arbitrumBroadcaster/transactionBroadcastRequest';

// Testing broadcast through vault rotations
export async function testRotationBarrier(testContext: TestContext) {
  const lpUri = (process.env.LP_URI || '//LP_1') as `//${string}`;
  const cf = await newChainflipIO(testContext.logger, {
    account: fullAccountFromUri(lpUri, 'LP'),
  });

  const withdrawalAddress = await newAssetAddress(Assets.ArbEth);

  await depositLiquidity(cf, Assets.ArbEth, 5);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  // Wait for the activation key to be created and the activation key to be sent for signing
  cf.info(`Vault rotation initiated`);
  await observeEvent(cf.logger, 'evmThresholdSigner:KeygenSuccess').event;
  cf.info(`Waiting for the bitcoin key handover`);
  await observeEvent(cf.logger, 'bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  cf.info(`Waiting for EVM key activation transaction to be sent for signing`);
  await observeEvent(cf.logger, 'evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvent(cf.logger, ':BroadcastAborted', {});

  cf.info(`Submitting withdrawal request.`);

  const depositAddressReadyEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityProvider.withdrawAsset(
        amountToFineAmount('2', assetDecimals(Assets.ArbEth)),
        Assets.ArbEth,
        {
          Arb: withdrawalAddress,
        },
      ),
    expectedEvent: {
      name: 'LiquidityProvider.WithdrawalEgressScheduled',
      schema: liquidityProviderWithdrawalEgressScheduled.refine(
        (event) => event.asset === Assets.ArbEth,
      ),
    },
  });

  const egressId = depositAddressReadyEvent.egressId;

  cf.info(
    `Withdrawal extrinsic included in a block, scheduled egress ID ${JSON.stringify(egressId)}`,
  );

  const batchBroadcastEvent = await cf.stepUntilEvent(
    'ArbitrumIngressEgress.BatchBroadcastRequested',
    arbitrumIngressEgressBatchBroadcastRequested.refine((event) =>
      JSON.stringify(event.egressIds).includes(JSON.stringify(egressId)),
    ),
  );

  const broadcastId = Number(batchBroadcastEvent.broadcastId);

  await cf.stepUntilEvent(
    'ArbitrumBroadcaster.TransactionBroadcastRequest',
    arbitrumBroadcasterTransactionBroadcastRequest.refine(
      (event) => Number(event.broadcastId) === broadcastId,
    ),
  );

  cf.info(`Broadcast requested for egress ID ${egressId}. Waiting for balance increase...`);

  await observeBalanceIncrease(cf.logger, Assets.ArbEth, withdrawalAddress);

  await broadcastAborted.stop();
}
