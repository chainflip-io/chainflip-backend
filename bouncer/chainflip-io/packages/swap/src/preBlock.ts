import { Prisma } from './client';
import type { Block } from './processBlocks';

const preBlock = async (txClient: Prisma.TransactionClient, block: Block) => {
  await txClient.swapDepositChannel.updateMany({
    where: {
      expiryBlock: { lte: block.height },
      isExpired: false,
    },
    data: { isExpired: true },
  });
};

export default preBlock;
