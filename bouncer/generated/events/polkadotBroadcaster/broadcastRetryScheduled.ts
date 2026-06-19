import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});

export const polkadotBroadcasterBroadcastRetryScheduledEvent = defineEvent(
  'PolkadotBroadcaster.BroadcastRetryScheduled',
  polkadotBroadcasterBroadcastRetryScheduled,
);
