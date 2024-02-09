// Set the column in the DB to the block timestamp and the deposit amount.
import assert from 'assert';
import { z } from 'zod';
import { chainflipAssetEnum, u128, u64 } from '@/shared/parsers';
import logger from '../utils/logger';
import { encodedAddress } from './common';
import type { EventHandlerArgs } from '.';

const depositChannelSwapOrigin = z.object({
  __kind: z.literal('DepositChannel'),
  channelId: u64,
  depositAddress: encodedAddress,
});
const vaultSwapOrigin = z.object({
  __kind: z.literal('Vault'),
  txHash: z.string(),
});

const swapScheduledArgs = z.object({
  swapId: u64,
  sourceAsset: chainflipAssetEnum,
  depositAmount: u128,
  destinationAsset: chainflipAssetEnum,
  destinationAddress: encodedAddress,
  origin: z.union([depositChannelSwapOrigin, vaultSwapOrigin]),
});

export type SwapScheduledEvent = z.input<typeof swapScheduledArgs>;

export default async function swapScheduled({
  prisma,
  block,
  event,
}: EventHandlerArgs): Promise<void> {
  const {
    swapId,
    sourceAsset,
    depositAmount,
    destinationAsset,
    destinationAddress,
    origin,
  } = swapScheduledArgs.parse(event.args);

  const newSwapData = {
    depositReceivedBlockIndex: `${block.height}-${event.indexInBlock}`,
    depositAmount: depositAmount.toString(),
    nativeId: swapId,
    depositReceivedAt: new Date(block.timestamp),
  };

  if (origin.__kind === 'DepositChannel') {
    const depositAddress = origin.depositAddress.address;

    const channels = await prisma.swapDepositChannel.findMany({
      where: {
        srcAsset: sourceAsset,
        depositAddress,
        expiryBlock: { gte: block.height },
        issuedBlock: { lte: block.height },
      },
    });
    if (channels.length === 0) {
      logger.info(
        `SwapScheduled: SwapDepositChannel not found for depositAddress ${depositAddress}`,
      );
      return;
    }
    assert(
      channels.length === 1,
      `SwapScheduled: too many active swap intents found for depositAddress ${depositAddress}`,
    );

    const [{ srcAsset, destAddress, destAsset, id }] = channels;

    await prisma.swap.create({
      data: {
        swapDepositChannelId: id,
        srcAsset,
        destAsset,
        destAddress,
        ...newSwapData,
      },
    });
  } else if (origin.__kind === 'Vault') {
    await prisma.swap.create({
      data: {
        srcAsset: sourceAsset,
        destAsset: destinationAsset,
        destAddress: destinationAddress.address,
        txHash: origin.txHash,
        ...newSwapData,
      },
    });
  }
}
