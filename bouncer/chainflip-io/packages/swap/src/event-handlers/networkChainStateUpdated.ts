import { z } from 'zod';
import { u128 } from '@/shared/parsers';
import { Chain } from '../enums';
import { EventHandlerArgs } from '.';

const numberOrHex = z.union([u128, z.number().transform((n) => BigInt(n))]);

const chainStateUpdatedArgs = z.object({
  newChainState: z.object({
    blockHeight: numberOrHex,
  }),
});

const chainStateUpdated =
  (chain: Chain) =>
  async ({ prisma, event, block }: EventHandlerArgs) => {
    const { blockHeight } = chainStateUpdatedArgs.parse(
      event.args,
    ).newChainState;

    await Promise.all([
      prisma.chainTracking.upsert({
        where: { chain },
        create: { chain, height: blockHeight, blockTrackedAt: block.timestamp },
        update: { height: blockHeight, blockTrackedAt: block.timestamp },
      }),
      prisma.swapDepositChannel.updateMany({
        where: {
          srcChain: chain,
          srcChainExpiryBlock: { lte: blockHeight },
          isExpired: false,
        },
        data: { isExpired: true },
      }),
    ]);
  };

export default chainStateUpdated;
