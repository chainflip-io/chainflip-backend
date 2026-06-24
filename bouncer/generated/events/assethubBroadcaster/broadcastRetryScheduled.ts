import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const assethubBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'AssethubBroadcaster.BroadcastRetryScheduled',
  assethubBroadcasterBroadcastRetryScheduled,
);
