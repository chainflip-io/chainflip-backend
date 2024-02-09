import { z } from 'zod';
import { u128, u64 } from '@/shared/parsers';
import type { EventHandlerArgs } from '.';

const swapExecutedArgs = z.object({
  swapId: u64,
  egressAmount: u128,
  intermediateAmount: u128.optional(),
});

export type SwapExecutedEvent = z.input<typeof swapExecutedArgs>;

export default async function swapExecuted({
  prisma,
  block,
  event,
}: EventHandlerArgs): Promise<void> {
  const { swapId, egressAmount, intermediateAmount } = swapExecutedArgs.parse(
    event.args,
  );

  // use updateMany to skip update if we are not tracking swap
  await prisma.swap.updateMany({
    where: { nativeId: swapId },
    data: {
      egressAmount: egressAmount.toString(),
      intermediateAmount: intermediateAmount?.toString(),
      swapExecutedAt: new Date(block.timestamp),
      swapExecutedBlockIndex: `${block.height}-${event.indexInBlock}`,
    },
  });
}
