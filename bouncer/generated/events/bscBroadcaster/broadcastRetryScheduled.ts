import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const bscBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'BscBroadcaster.BroadcastRetryScheduled',
  bscBroadcasterBroadcastRetryScheduled,
);
