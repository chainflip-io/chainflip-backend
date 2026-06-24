import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const bitcoinBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'BitcoinBroadcaster.BroadcastRetryScheduled',
  bitcoinBroadcasterBroadcastRetryScheduled,
);
