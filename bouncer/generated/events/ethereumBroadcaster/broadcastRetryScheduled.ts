import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const ethereumBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'EthereumBroadcaster.BroadcastRetryScheduled',
  ethereumBroadcasterBroadcastRetryScheduled,
);
