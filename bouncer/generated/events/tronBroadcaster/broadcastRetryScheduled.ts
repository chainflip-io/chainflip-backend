import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const tronBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'TronBroadcaster.BroadcastRetryScheduled',
  tronBroadcasterBroadcastRetryScheduled,
);
