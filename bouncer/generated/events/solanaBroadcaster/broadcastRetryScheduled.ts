import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const solanaBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'SolanaBroadcaster.BroadcastRetryScheduled',
  solanaBroadcasterBroadcastRetryScheduled,
);
