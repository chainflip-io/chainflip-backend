import assert from 'assert';
import express from 'express';
import { assetChains, Chain } from '@/shared/enums';
import BrokerClient from '@/shared/node-apis/broker';
import { postSwapSchema } from '@/shared/schemas';
import { validateAddress } from '@/shared/validation/addressValidation';
import prisma, {
  Egress,
  Swap,
  SwapDepositChannel,
  Broadcast,
} from '../../client';
import { isProduction } from '../../utils/consts';
import { handleExit } from '../../utils/function';
import logger from '../../utils/logger';
import ServiceError from '../../utils/ServiceError';
import { asyncHandler } from '../common';

const router = express.Router();

export enum State {
  Complete = 'COMPLETE',
  BroadcastAborted = 'BROADCAST_ABORTED',
  BroadcastRequested = 'BROADCAST_REQUESTED',
  EgressScheduled = 'EGRESS_SCHEDULED',
  SwapExecuted = 'SWAP_EXECUTED',
  DepositReceived = 'DEPOSIT_RECEIVED',
  AwaitingDeposit = 'AWAITING_DEPOSIT',
}

type SwapWithBroadcast = Swap & {
  egress:
    | (Egress & {
        broadcast: Broadcast | null;
      })
    | null;
};

const channelIdRegex =
  /^(?<issuedBlock>\d+)-(?<srcChain>[a-z]+)-(?<channelId>\d+)$/i;
const swapIdRegex = /^\d+$/i;
const txHashRegex = /^0x[a-f\d]+$/i;

router.get(
  '/:id',
  asyncHandler(async (req, res) => {
    const { id } = req.params;

    let swap: SwapWithBroadcast | null | undefined;
    let swapDepositChannel:
      | (SwapDepositChannel & { swaps: SwapWithBroadcast[] })
      | null
      | undefined;

    if (channelIdRegex.test(id)) {
      const { issuedBlock, srcChain, channelId } =
        channelIdRegex.exec(id)!.groups!; // eslint-disable-line @typescript-eslint/no-non-null-assertion

      swapDepositChannel = await prisma.swapDepositChannel.findUnique({
        where: {
          issuedBlock_srcChain_channelId: {
            issuedBlock: Number(issuedBlock),
            srcChain: srcChain as Chain,
            channelId: BigInt(channelId),
          },
        },
        include: {
          swaps: { include: { egress: { include: { broadcast: true } } } },
        },
      });

      if (!swapDepositChannel) {
        logger.info(`could not find swap request with id "${id}`);
        throw ServiceError.notFound();
      }

      swap = swapDepositChannel.swaps.at(0);
    } else if (swapIdRegex.test(id)) {
      swap = await prisma.swap.findUnique({
        where: { nativeId: BigInt(id) },
        include: { egress: { include: { broadcast: true } } },
      });
    } else if (txHashRegex.test(id)) {
      swap = await prisma.swap.findFirst({
        where: { txHash: id },
        include: { egress: { include: { broadcast: true } } },
        // just get the last one for now
        orderBy: { createdAt: 'desc' },
      });
    }

    ServiceError.assert(
      swapDepositChannel || swap,
      'notFound',
      'resource not found',
    );

    let state: State;

    if (swap?.egress?.broadcast?.succeededAt) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.Complete;
    } else if (swap?.egress?.broadcast?.abortedAt) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.BroadcastAborted;
    } else if (swap?.egress?.broadcast) {
      assert(swap.swapExecutedAt, 'swapExecutedAt should not be null');
      state = State.BroadcastRequested;
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

    const readField = <T extends keyof Swap & keyof SwapDepositChannel>(
      field: T,
    ) =>
      (swap && swap[field]) ??
      (swapDepositChannel && swapDepositChannel[field]);

    const srcAsset = readField('srcAsset');
    const destAsset = readField('destAsset');

    const response = {
      state,
      srcChain: srcAsset && assetChains[srcAsset],
      destChain: destAsset && assetChains[destAsset],
      srcAsset,
      destAsset,
      destAddress: readField('destAddress'),
      depositAddress: swapDepositChannel?.depositAddress,
      expectedDepositAmount:
        swapDepositChannel?.expectedDepositAmount.toString(),
      swapId: swap?.nativeId.toString(),
      depositAmount: swap?.depositAmount?.toString(),
      depositReceivedAt: swap?.depositReceivedAt.valueOf(),
      depositReceivedBlockIndex: swap?.depositReceivedBlockIndex,
      swapExecutedAt: swap?.swapExecutedAt?.valueOf(),
      swapExecutedBlockIndex: swap?.swapExecutedBlockIndex,
      egressAmount: swap?.egress?.amount?.toString(),
      egressScheduledAt: swap?.egress?.scheduledAt?.valueOf(),
      egressScheduledBlockIndex: swap?.egress?.scheduledBlockIndex,
      broadcastRequestedAt: swap?.egress?.broadcast?.requestedAt?.valueOf(),
      broadcastRequestedBlockIndex:
        swap?.egress?.broadcast?.requestedBlockIndex,
      broadcastAbortedAt: swap?.egress?.broadcast?.abortedAt?.valueOf(),
      broadcastAbortedBlockIndex: swap?.egress?.broadcast?.abortedBlockIndex,
      broadcastSucceededAt: swap?.egress?.broadcast?.succeededAt?.valueOf(),
      broadcastSucceededBlockIndex:
        swap?.egress?.broadcast?.succeededBlockIndex,
    };

    logger.info('sending response for swap request', { id, response });

    res.json(response);
  }),
);

let client: BrokerClient | undefined;

router.post(
  '/',
  asyncHandler(async (req, res) => {
    const result = postSwapSchema.safeParse(req.body);
    if (!result.success) {
      logger.info('received bad request for new swap', { body: req.body });
      throw ServiceError.badRequest('invalid request body');
    }

    const payload = result.data;

    if (
      !validateAddress(payload.destAsset, payload.destAddress, isProduction)
    ) {
      throw ServiceError.badRequest('provided address is not valid');
    }

    if (!client) {
      client = await BrokerClient.create({ logger });
      handleExit(() => client?.close());
    }

    const { address: depositAddress, ...blockInfo } =
      await client.requestSwapDepositAddress(payload);

    const { destChain, ...rest } = payload;

    const { issuedBlock, expiryBlock, srcChain, channelId } =
      await prisma.swapDepositChannel.create({
        data: {
          ...rest,
          depositAddress,
          ...blockInfo,
        },
      });

    res.json({
      id: `${issuedBlock}-${srcChain}-${channelId}`,
      depositAddress,
      issuedBlock,
      expiryBlock,
    });
  }),
);

export default router;
