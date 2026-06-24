import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const arbitrumBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'ArbitrumBroadcaster.BroadcastRetryScheduled',
  arbitrumBroadcasterBroadcastRetryScheduled,
);
