import { z } from 'zod';
import { assetChains } from '@/shared/enums';
import { unsignedInteger, chainflipAssetEnum } from '@/shared/parsers';
import { egressId } from './common';
import type { EventHandlerArgs } from '.';

const eventArgs = z.object({
  id: egressId,
  asset: chainflipAssetEnum,
  amount: unsignedInteger,
});

/**
 * @deprecated no longer exists since 1.2.0
 *
 * the event emits the egress id (Network, number) and the egress amount. the
 * egress id is used to uniquely identify an egress and correlate it to a swap
 * and determining if funds were successfully sent by the broadcast pallets
 */
export default async function networkEgressScheduled({
  prisma,
  block,
  event,
}: EventHandlerArgs): Promise<void> {
  const { id, asset, amount } = eventArgs.parse(event.args);

  await prisma.egress.create({
    data: {
      nativeId: id[1],
      chain: assetChains[asset],
      amount: amount.toString(),
      scheduledAt: new Date(block.timestamp),
      scheduledBlockIndex: `${block.height}-${event.indexInBlock}`,
    },
  });
}
