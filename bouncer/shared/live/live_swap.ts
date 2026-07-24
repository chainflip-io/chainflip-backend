import Web3 from 'web3';
import {
  Asset,
  amountToFineAmountBigInt,
  chainFromAsset,
  encodedAddress,
  getEvmWhaleKeypair,
  getSwapRate,
  isEvmChain,
  SwapRequestType,
  TransactionOrigin,
  observeSwapRequested,
} from 'shared/utils';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { getChainflipApi } from 'shared/utils/substrate';
import { SwapParams, waitForBroadcastOutcome } from 'shared/perform_swap';
import { DcaParams } from 'shared/new_swap';
import type { CfChainsRefundParametersChannelRefundParameters } from 'generated/chaintypes/chainflip-node';
import { z } from 'zod';
import { swappingSwapDepositAddressReadyEvent } from 'generated/events/swapping/swapDepositAddressReady';
import { swappingSwapExecutedEvent } from 'generated/events/swapping/swapExecuted';
import {
  swappingSwapEgressScheduled,
  swappingSwapEgressScheduledEvent,
} from 'generated/events/swapping/swapEgressScheduled';
import { swappingRefundEgressScheduledEvent } from 'generated/events/swapping/refundEgressScheduled';
import { swappingSwapRequestCompletedEvent } from 'generated/events/swapping/swapRequestCompleted';
import { accountRolesAccountRoleRegisteredEvent } from 'generated/events/accountRoles/accountRoleRegistered';
import { LiveSwapReport } from 'shared/live/report';

/** The external wallet we control on the asset's chain (from the whale key env vars). */
export function ourExternalWallet(asset: Asset): string {
  const chain = chainFromAsset(asset);
  if (!isEvmChain(chain)) {
    throw new Error(
      `Only EVM assets are supported by the live commands for now (got ${asset} on ${chain})`,
    );
  }
  const { privkey } = getEvmWhaleKeypair(chain);
  return new Web3().eth.accounts.privateKeyToAccount(privkey).address;
}

/**
 * Checks that the account is a registered broker, optionally registering it. Unlike
 * `setupAccount` this never tries to fund the account: on a live network it must already
 * hold enough FLIP to register.
 */
export async function ensureBrokerRole(
  cf: ChainflipIO<WithBrokerAccount>,
  registerIfNeeded: boolean,
) {
  const address = cf.requirements.account.keypair.address;
  await using client = await getChainflipApi();
  const role = (await client.query.accountRoles.accountRoles(address)) ?? 'Unregistered';

  if (role === 'Broker') {
    return;
  }
  if (role !== 'Unregistered') {
    throw new Error(`Account ${address} has role '${role}', expected 'Broker'`);
  }
  if (!registerIfNeeded) {
    throw new Error(
      `Account ${address} is not registered as a broker. ` +
        `Fund it with FLIP and re-run with --register-broker.`,
    );
  }
  cf.info(`Registering ${address} as a broker`);
  await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.swapping.registerAsBroker(),
    expectedEvent: accountRolesAccountRoleRegisteredEvent.refine(
      (event) => event.accountId === address,
    ),
  });
}

/**
 * Derives a fill-or-kill minimum price (X128, fine-output per fine-input) from the current
 * quoted rate minus a tolerance. If the swap can't achieve at least that price it refunds to
 * our own wallet instead of executing at whatever liquidity happens to be available.
 */
export async function computeFokMinPrice(
  sourceAsset: Asset,
  destAsset: Asset,
  amount: string,
  toleranceBps: number,
): Promise<{ minPriceX128: bigint; quotedOutput: string }> {
  const quotedOutput = await getSwapRate(sourceAsset, destAsset, amount);
  const fineInput = amountToFineAmountBigInt(amount, sourceAsset);
  const fineOutput = amountToFineAmountBigInt(quotedOutput, destAsset);
  const minFineOutput = (fineOutput * BigInt(10000 - toleranceBps)) / 10000n;
  return { minPriceX128: (minFineOutput * 2n ** 128n) / fineInput, quotedOutput };
}

export type LiveDepositChannel = {
  swapParams: SwapParams;
  sourceChainExpiryBlock: bigint;
};

/**
 * Opens a swap deposit channel via a direct `Swapping.request_swap_deposit_address` extrinsic
 * signed by our own broker account.
 */
export async function requestLiveDepositChannel(
  cf: ChainflipIO<WithBrokerAccount>,
  args: {
    sourceAsset: Asset;
    destAsset: Asset;
    destAddress: string;
    refundAddress: string;
    minPriceX128: bigint;
    retryDurationBlocks: number;
    dcaParams?: DcaParams;
  },
): Promise<LiveDepositChannel> {
  const sourceChain = chainFromAsset(args.sourceAsset);
  const destChain = chainFromAsset(args.destAsset);
  const refundParameters: CfChainsRefundParametersChannelRefundParameters = {
    retryDuration: args.retryDurationBlocks,
    refundAddress: encodedAddress(sourceChain, args.refundAddress),
    minPrice: args.minPriceX128,
    refundCcmMetadata: undefined,
    // Our JIT orders are placed at the oracle price (see live_jit.ts), so the swap stays inside the
    // default oracle price protection (LPP). Leave it at the chain default; LIVE_MAX_ORACLE_SLIPPAGE_BPS
    // can override it if a pool's oracle is unavailable/stale.
    maxOraclePriceSlippage: process.env.LIVE_MAX_ORACLE_SLIPPAGE_BPS
      ? Number(process.env.LIVE_MAX_ORACLE_SLIPPAGE_BPS)
      : undefined,
  };

  const addressReady = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.swapping.requestSwapDepositAddressWithAffiliates(
        args.sourceAsset,
        args.destAsset,
        encodedAddress(destChain, args.destAddress),
        0, // broker commission
        undefined, // channel metadata
        0, // boost fee
        [], // affiliate fees
        refundParameters,
        args.dcaParams && {
          numberOfChunks: args.dcaParams.numberOfChunks,
          chunkInterval: args.dcaParams.chunkIntervalBlocks,
        },
      ),
    expectedEvent: swappingSwapDepositAddressReadyEvent.refine(
      (event) =>
        event.sourceAsset === args.sourceAsset &&
        event.destinationAsset === args.destAsset &&
        event.destinationAddress.address.toLowerCase() === args.destAddress.toLowerCase(),
    ),
  });

  return {
    swapParams: {
      sourceAsset: args.sourceAsset,
      destAsset: args.destAsset,
      depositAddress: addressReady.depositAddress.address,
      destAddress: args.destAddress,
      channelId: Number(addressReady.channelId),
    },
    sourceChainExpiryBlock: BigInt(addressReady.sourceChainExpiryBlock),
  };
}

export type TrackedSwap = NonNullable<LiveSwapReport['swap']> & {
  outcome: LiveSwapReport['outcome'];
};

/**
 * Follows a swap from SwapRequested through to broadcast success (or refund), collecting the
 * milestone data for the report. All event lookups go through the indexer via ChainflipIO,
 * which the live setup syncs from the target network.
 */
export async function trackSwapToCompletion<A>(
  cf: ChainflipIO<A>,
  swapParams: SwapParams,
  dcaChunks: number,
): Promise<TrackedSwap> {
  const requested = await observeSwapRequested(
    cf,
    swapParams.sourceAsset,
    swapParams.destAsset,
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );
  const swapRequestId = BigInt(requested.swapRequestId);
  cf.info(`Swap requested with id ${swapRequestId}`);

  const tracked: TrackedSwap = {
    swapRequestId: swapRequestId.toString(),
    dcaChunks,
    executed: [],
    outcome: 'incomplete',
  };
  const seenSwapIds = new Set<bigint>();
  let egressId: z.infer<typeof swappingSwapEgressScheduled>['egressId'] | undefined;
  let refundEgressId: typeof egressId;

  let completed = false;
  while (!completed) {
    const milestones = await cf.stepUntilAnyEventsOf({
      executed: swappingSwapExecutedEvent.refine(
        (event) =>
          BigInt(event.swapRequestId) === swapRequestId && !seenSwapIds.has(BigInt(event.swapId)),
      ),
      egressScheduled: swappingSwapEgressScheduledEvent.refine(
        (event) => BigInt(event.swapRequestId) === swapRequestId && tracked.egress === undefined,
      ),
      refundEgressScheduled: swappingRefundEgressScheduledEvent.refine(
        (event) =>
          BigInt(event.swapRequestId) === swapRequestId && tracked.refundEgress === undefined,
      ),
      completed: swappingSwapRequestCompletedEvent.refine(
        (event) => BigInt(event.swapRequestId) === swapRequestId,
      ),
    });

    for (const milestone of milestones) {
      switch (milestone.key) {
        case 'executed': {
          const event = milestone.data;
          seenSwapIds.add(BigInt(event.swapId));
          tracked.executed.push({
            swapId: event.swapId.toString(),
            inputAmount: event.inputAmount.toString(),
            intermediateAmount: event.intermediateAmount?.toString(),
            outputAmount: event.outputAmount.toString(),
            networkFee: event.networkFee.toString(),
            brokerFee: event.brokerFee.toString(),
            oracleDelta: event.oracleDelta ?? undefined,
          });
          cf.info(
            `Chunk ${tracked.executed.length}/${dcaChunks} executed (swap id ${event.swapId})`,
          );
          break;
        }
        case 'egressScheduled':
          egressId = milestone.data.egressId;
          tracked.egress = {
            egressId: egressId.join('-'),
            amount: milestone.data.amount.toString(),
            fee: milestone.data.egressFee[0].toString(),
            broadcastSuccess: false,
          };
          cf.info(`Egress scheduled: ${tracked.egress.egressId}`);
          break;
        case 'refundEgressScheduled':
          refundEgressId = milestone.data.egressId;
          tracked.refundEgress = {
            egressId: refundEgressId.join('-'),
            amount: milestone.data.amount.toString(),
          };
          cf.warn(`Refund egress scheduled: ${tracked.refundEgress.egressId} (fill-or-kill hit)`);
          break;
        case 'completed':
          completed = true;
          break;
        default:
          break;
      }
    }
  }

  // The output (or the refund) still has to reach the external chain.
  if (egressId !== undefined) {
    tracked.egress!.broadcastId = await waitForBroadcastOutcome(cf, swapParams.destAsset, egressId);
    tracked.egress!.broadcastSuccess = true;
    tracked.outcome = 'success';
  } else if (refundEgressId !== undefined) {
    tracked.refundEgress!.broadcastId = await waitForBroadcastOutcome(
      cf,
      swapParams.sourceAsset,
      refundEgressId,
    );
    tracked.outcome = 'refunded';
  }
  // If neither egress happened, leave the outcome as 'incomplete'.
  return tracked;
}
