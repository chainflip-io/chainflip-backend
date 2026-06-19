import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const arbitrumBroadcasterBroadcastAbortedEvent = defineEvent(
  'ArbitrumBroadcaster.BroadcastAborted',
  arbitrumBroadcasterBroadcastAborted,
);
