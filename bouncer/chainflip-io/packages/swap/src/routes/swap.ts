import assert from 'assert';
import express from 'express';
import { assetChains } from '@/shared/enums';
import { openSwapDepositChannelSchema } from '@/shared/schemas';
import { screamingSnakeToPascalCase } from '@/shared/strings';
import { asyncHandler } from './common';
import prisma, {
  Egress,
  Swap,
  SwapDepositChannel,
  Broadcast,
  SwapFee,
  FailedSwap,
} from '../client';
import openSwapDepositChannel from '../handlers/openSwapDepositChannel';
import {
  getPendingBroadcast,
  getPendingDeposit,
} from '../ingress-egress-tracking';
import logger from '../utils/logger';
import ServiceError from '../utils/ServiceError';

const router = express.Router();

export enum State {
  Failed = 'FAILED',
  Complete = 'COMPLETE',
  BroadcastAborted = 'BROADCAST_ABORTED',
  Broadcasted = 'BROADCASTED',
  BroadcastRequested = 'BROADCAST_REQUESTED',
  EgressScheduled = 'EGRESS_SCHEDULED',
  SwapExecuted = 'SWAP_EXECUTED',
  DepositReceived = 'DEPOSIT_RECEIVED',
  AwaitingDeposit = 'AWAITING_DEPOSIT',
}

type SwapWithBroadcastAndFees = Swap & {
  fees: SwapFee[];
  egress:
    | (Egress & {
        broadcast: Broadcast | null;
      })
    | null;
};

const coerceChain = (chain: string) => {
  const uppercase = chain.toUpperCase();
  switch (uppercase) {
    case 'BITCOIN':
    case 'ETHEREUM':
    case 'POLKADOT':
      return screamingSnakeToPascalCase(uppercase);
    default:
      throw ServiceError.badRequest(`invalid chain "${chain}"`);
  }
};

const channelIdRegex =
  /^(?<issuedBlock>\d+)-(?<srcChain>[a-z]+)-(?<channelId>\d+)$/i;
const swapIdRegex = /^\d+$/i;
const txHashRegex = /^0x[a-f\d]+$/i;

// eslint-disable-next-line @typescript-eslint/ban-types
const readField = <A extends {}, B extends {}, K extends keyof A & keyof B>(
  a: A | null | undefined,
  b: B | null | undefined,
  key: K,
) => a?.[key] ?? b?.[key];

router.get(
  '/:id',
  asyncHandler(async (req, res) => {
    const { id } = req.params;

    let swap: SwapWithBroadcastAndFees | null | undefined;
    let failedSwap: FailedSwap | null | undefined;
    let swapDepositChannel:
      | (SwapDepositChannel & {
          swaps: SwapWithBroadcastAndFees[];
          failedSwaps: FailedSwap[];
        })
      | null
      | undefined;

    if (channelIdRegex.test(id)) {
      const { issuedBlock, srcChain, channelId } =
        channelIdRegex.exec(id)!.groups!;

      swapDepositChannel = await prisma.swapDepositChannel.findUnique({
        where: {
          issuedBlock_srcChain_channelId: {
            issuedBlock: Number(issuedBlock),
            srcChain: coerceChain(srcChain),
            channelId: BigInt(channelId),
          },
        },
        include: {
          swaps: {
            include: { egress: { include: { broadcast: true } }, fees: true },
          },
          failedSwaps: true,
        },
      });

      if (!swapDepositChannel) {
        logger.info(`could not find swap request with id "${id}`);
        throw ServiceError.notFound();
      }

      swap = swapDepositChannel.swaps.at(0);
      failedSwap = swapDepositChannel.failedSwaps.at(0);
    } else if (swapIdRegex.test(id)) {
      swap = await prisma.swap.findUnique({
        where: { nativeId: BigInt(id) },
        include: { egress: { include: { broadcast: true } }, fees: true },
      });
    } else if (txHashRegex.test(id)) {
      swap = await prisma.swap.findFirst({
        where: { txHash: id },
        include: { egress: { include: { broadcast: true } }, fees: true },
        // just get the last one for now
        orderBy: { nativeId: 'desc' },
      });
    }

    ServiceError.assert(
      swapDepositChannel || swap,
      'notFound',
      'resource not found',
    );

    let state: State;

    if (failedSwap) {
      state = State.Failed;
    } else if (swap?.egress?.broadcast?.succeededAt) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.Complete;
    } else if (swap?.egress?.broadcast?.abortedAt) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.BroadcastAborted;
    } else if (swap?.egress?.broadcast) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      if (await getPendingBroadcast(swap.egress.broadcast)) {
        state = State.Broadcasted;
      } else {
        state = State.BroadcastRequested;
      }
    } else if (swap?.egress) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.EgressScheduled;
    } else if (swap?.swapExecutedAt) {
      state = State.SwapExecuted;
    } else if (swap?.depositReceivedAt) {
      state = State.DepositReceived;
    } else {
      state = State.AwaitingDeposit;
    }

    const srcAsset = readField(swap, swapDepositChannel, 'srcAsset');
    const destAsset = readField(swap, swapDepositChannel, 'destAsset');

    let pendingDeposit;
    if (
      srcAsset &&
      state === State.AwaitingDeposit &&
      swapDepositChannel?.depositAddress
    ) {
      pendingDeposit = await getPendingDeposit(
        assetChains[srcAsset],
        srcAsset,
        swapDepositChannel.depositAddress,
      );
    }

    let ccmMetadata;
    if (readField(swap, swapDepositChannel, 'ccmGasBudget')) {
      ccmMetadata = {
        gasBudget: readField(
          swap,
          swapDepositChannel,
          'ccmGasBudget',
        )?.toFixed(),
        message: readField(swap, swapDepositChannel, 'ccmMessage'),
      };
    }

    const response = {
      state,
      type: swap?.type,
      srcChain: srcAsset && assetChains[srcAsset],
      destChain: destAsset && assetChains[destAsset],
      srcAsset,
      destAsset,
      destAddress: readField(swap, swapDepositChannel, 'destAddress'),
      depositChannelCreatedAt: swapDepositChannel?.createdAt.valueOf(),
      depositChannelBrokerCommissionBps:
        swapDepositChannel?.brokerCommissionBps,
      depositAddress: swapDepositChannel?.depositAddress,
      expectedDepositAmount:
        swapDepositChannel?.expectedDepositAmount?.toFixed(),
      swapId: swap?.nativeId.toString(),
      depositAmount: swap?.depositAmount?.toFixed() ?? pendingDeposit?.amount,
      depositTransactionHash: pendingDeposit?.transactionHash,
      depositTransactionConfirmations: pendingDeposit?.transactionConfirmations,
      depositReceivedAt: swap?.depositReceivedAt.valueOf(),
      depositReceivedBlockIndex: swap?.depositReceivedBlockIndex,
      intermediateAmount: swap?.intermediateAmount?.toFixed(),
      swapExecutedAt: swap?.swapExecutedAt?.valueOf(),
      swapExecutedBlockIndex: swap?.swapExecutedBlockIndex,
      egressAmount: swap?.egress?.amount?.toFixed(),
      egressScheduledAt: swap?.egress?.scheduledAt?.valueOf(),
      egressScheduledBlockIndex: swap?.egress?.scheduledBlockIndex,
      feesPaid:
        swap?.fees.map((fee) => ({
          type: fee.type,
          chain: assetChains[fee.asset],
          asset: fee.asset,
          amount: fee.amount.toFixed(),
        })) ?? [],
      broadcastRequestedAt: swap?.egress?.broadcast?.requestedAt?.valueOf(),
      broadcastRequestedBlockIndex:
        swap?.egress?.broadcast?.requestedBlockIndex,
      broadcastAbortedAt: swap?.egress?.broadcast?.abortedAt?.valueOf(),
      broadcastAbortedBlockIndex: swap?.egress?.broadcast?.abortedBlockIndex,
      broadcastSucceededAt: swap?.egress?.broadcast?.succeededAt?.valueOf(),
      broadcastSucceededBlockIndex:
        swap?.egress?.broadcast?.succeededBlockIndex,
      depositChannelExpiryBlock:
        swapDepositChannel?.srcChainExpiryBlock?.toString(),
      estimatedDepositChannelExpiryTime:
        swapDepositChannel?.estimatedExpiryAt?.valueOf(),
      isDepositChannelExpired: swapDepositChannel?.isExpired ?? false,
      ccmDepositReceivedBlockIndex: swap?.ccmDepositReceivedBlockIndex,
      ccmMetadata,
      depositChannelOpenedThroughBackend:
        swapDepositChannel?.openedThroughBackend ?? false,
      failed: failedSwap && {
        type: failedSwap.type,
        reason: failedSwap.reason,
      },
    };

    logger.info('sending response for swap request', { id, response });

    res.json(response);
  }),
);

// TODO(major): remove this handler. it's replaced by tRPC
router.post(
  '/',
  asyncHandler(async (req, res) => {
    const result = openSwapDepositChannelSchema.safeParse(req.body);
    if (!result.success) {
      logger.info('received bad request for new swap', { body: req.body });
      throw ServiceError.badRequest('invalid request body');
    }

    const { srcChainExpiryBlock, ...response } = await openSwapDepositChannel(
      result.data,
    );

    res.json(response);
  }),
);

export default router;
