import { z } from 'zod';
import { Chain } from '@/shared/enums';
import { number } from '@/shared/parsers';
import { EventHandlerArgs } from '.';

const thresholdSignatureInvalidArgs = z.object({
  broadcastId: number,
  retryBroadcastId: number.optional(),
});

const networkThresholdSignatureInvalid =
  (chain: Chain) =>
  async ({ prisma, event, block }: EventHandlerArgs) => {
    const { broadcastId, retryBroadcastId } =
      thresholdSignatureInvalidArgs.parse(event.args);
    if (retryBroadcastId === undefined) return;

    const broadcast = await prisma.broadcast.findUnique({
      where: {
        nativeId_chain: {
          chain,
          nativeId: broadcastId,
        },
      },
      include: { egresses: true },
    });
    if (!broadcast) return;

    const newBroadcast = await prisma.broadcast.create({
      data: {
        nativeId: retryBroadcastId,
        chain: broadcast.chain,
        type: broadcast.type,
        requestedBlockIndex: `${block.height}-${event.indexInBlock}`,
        requestedAt: new Date(block.timestamp),
      },
      select: { id: true },
    });

    await Promise.all([
      prisma.broadcast.update({
        where: { id: broadcast.id },
        data: { replacedById: newBroadcast.id },
      }),
      prisma.egress.updateMany({
        where: { id: { in: broadcast.egresses.map(({ id }) => id) } },
        data: { broadcastId: newBroadcast.id },
      }),
    ]);
  };

export default networkThresholdSignatureInvalid;
