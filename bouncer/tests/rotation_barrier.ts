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
  const withdrawalAddress = await newAssetAddress(InternalAssets.Eth);

  await depositLiquidity(logger, InternalAssets.Eth, 5, false, lpUri);

  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  // Wait for the activation key to be created and the activation key to be sent for signing
  logger.info(`Vault rotation initiated`);
  await observeEvent(logger, 'evmThresholdSigner:KeygenSuccess').event;
  logger.info(`Waiting for the bitcoin key handover`);
  await observeEvent(logger, 'bitcoinThresholdSigner:KeyHandoverSuccessReported').event;
  logger.info(`Waiting for eth key activation transaction to be sent for signing`);
  await observeEvent(logger, 'evmThresholdSigner:ThresholdSignatureRequest').event;

  const broadcastAborted = observeBadEvent(logger, ':BroadcastAborted', {});

  logger.info(`Submitting withdrawal request.`);
  const api = await getChainflipApi();
  const { promise, waiter } = waitForExt(api, logger, 'InBlock', await lpMutex.acquire(lpUri));
  const lp = createStateChainKeypair(lpUri);
  const nonce = Number(await api.rpc.system.accountNextIndex(lp.address));
  const unsub = await api.tx.liquidityProvider
    .withdrawAsset(amountToFineAmount('2', assetDecimals(InternalAssets.Eth)), InternalAssets.Eth, {
      Eth: withdrawalAddress,
    })
    .signAndSend(lp, { nonce }, waiter);

  await promise;
  unsub();

  logger.info(`Withdrawal extrinsic included in a block`);

  // Override the default 120s timeout to account for the key rotation.
  await observeBalanceIncrease(logger, InternalAssets.Eth, withdrawalAddress, undefined, 240);

  await broadcastAborted.stop();
}
