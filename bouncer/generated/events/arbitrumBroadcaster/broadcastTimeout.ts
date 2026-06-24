import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const arbitrumBroadcasterBroadcastTimeoutEvent = defineEvent(
  'ArbitrumBroadcaster.BroadcastTimeout',
  arbitrumBroadcasterBroadcastTimeout,
);
