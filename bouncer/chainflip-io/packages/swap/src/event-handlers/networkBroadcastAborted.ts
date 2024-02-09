import { z } from 'zod';
import { Chain } from '@/shared/enums';
import { unsignedInteger } from '@/shared/parsers';
import { EventHandlerArgs } from './index';

const eventArgs = z.object({
  broadcastId: unsignedInteger,
});

export async function handleEvent(
  chain: Chain,
  { prisma, block, event }: EventHandlerArgs,
): Promise<void> {
  const { broadcastId } = eventArgs.parse(event.args);

  // use updateMany to skip update if we are not tracking swap
  await prisma.broadcast.updateMany({
    where: { chain, nativeId: broadcastId },
    data: {
      abortedAt: new Date(block.timestamp),
      abortedBlockIndex: `${block.height}-${event.indexInBlock}`,
    },
  });
}

export default function networkBroadcastAborted(
  chain: Chain,
): (args: EventHandlerArgs) => Promise<void> {
  return (args: EventHandlerArgs) => handleEvent(chain, args);
}
