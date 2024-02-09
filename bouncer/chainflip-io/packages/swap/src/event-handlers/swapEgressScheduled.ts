import { z } from 'zod';
import { chainflipChain, unsignedInteger } from '@/shared/parsers';
import { Prisma } from '../client';
import type { EventHandlerArgs } from '.';

const eventArgs = z.object({
  swapId: unsignedInteger,
  egressId: z.tuple([
    z.object({ __kind: chainflipChain }).transform(({ __kind }) => __kind),
    unsignedInteger,
  ]),
});

/**
 * this event is emitted in order to correlate the egress id from a network
 * deposit/egress pallet to a swap id
 */
export default async function swapEgressScheduled({
  prisma,
  event,
}: EventHandlerArgs): Promise<void> {
  try {
    const {
      swapId,
      egressId: [chain, nativeId],
    } = eventArgs.parse(event.args);

    await prisma.swap.update({
      where: { nativeId: swapId },
      data: {
        egress: { connect: { nativeId_chain: { chain, nativeId } } },
      },
    });
  } catch (error) {
    if (
      error instanceof Prisma.PrismaClientKnownRequestError &&
      error.code === 'P2025'
    ) {
      // we tried to update a swap that doesn't exist. we can ignore this error
      // because we are not tracking every swap, only swaps originated by swap
      // intents that we already have in the db
      return;
    }
    throw error;
  }
}
