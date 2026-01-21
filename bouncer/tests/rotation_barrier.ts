import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { TestContext } from 'shared/utils/test_context';
import { observeEvent, observeBadEvent, getChainflipApi } from 'shared/utils/substrate';
import { depositLiquidity } from 'shared/deposit_liquidity';
import {
  amountToFineAmount,
  assetDecimals,
  Assets,
  createStateChainKeypair,
  cfMutex,
  newAssetAddress,
  observeBalanceIncrease,
  waitForExt,
} from 'shared/utils';
import { fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';

// Testing broadcast through vault rotations
export async function testRotationBarrier(testContext: TestContext) {
  const lpUri = (process.env.LP_URI || '//LP_1') as `//${string}`;
  const withdrawalAddress = await newAssetAddress(Assets.ArbEth);

  const cf = await newChainflipIO(testContext.logger, {
    account: fullAccountFromUri(lpUri, 'LP'),
  });

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
  const api = await getChainflipApi();
  const { promise, waiter } = waitForExt(api, cf.logger, 'InBlock', await cfMutex.acquire(lpUri));
  const lp = createStateChainKeypair(lpUri);
  const nonce = Number(await api.rpc.system.accountNextIndex(lp.address));
  const unsub = await api.tx.liquidityProvider
    .withdrawAsset(amountToFineAmount('2', assetDecimals(Assets.ArbEth)), Assets.ArbEth, {
      Arb: withdrawalAddress,
    })
    .signAndSend(lp, { nonce }, waiter);

  const events = (await promise).events;
  unsub();

  const egressId = events
    .find(({ event }) => event.method.endsWith('WithdrawalEgressScheduled'))
    ?.event.data[0].toHuman();

  cf.info(
    `Withdrawal extrinsic included in a block, scheduled egress ID ${JSON.stringify(egressId)}`,
  );

  const event = await observeEvent(cf.logger, 'arbitrumIngressEgress:BatchBroadcastRequested', {
    test: (e) => JSON.stringify(e.data.egressIds).includes(JSON.stringify(egressId)),
    historicalCheckBlocks: 10,
  }).event;

  const broadcastId = Number(event.data.broadcastId);

  await observeEvent(cf.logger, 'arbitrumBroadcaster:TransactionBroadcastRequest', {
    test: (e) => Number(e.data.broadcastId) === broadcastId,
    historicalCheckBlocks: 10,
  }).event;

  cf.info(`Broadcast requested for egress ID ${egressId}. Waiting for balance increase...`);

  await observeBalanceIncrease(cf.logger, Assets.ArbEth, withdrawalAddress);

  await broadcastAborted.stop();
}
