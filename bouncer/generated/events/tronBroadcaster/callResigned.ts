import { z } from 'zod';
import { cfChainsTronTronTransaction } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterCallResigned = z.object({
  broadcastId: z.number(),
  transactionPayload: cfChainsTronTronTransaction,
});

export const tronBroadcasterCallResignedEvent = defineEvent(
  'TronBroadcaster.CallResigned',
  tronBroadcasterCallResigned,
);
