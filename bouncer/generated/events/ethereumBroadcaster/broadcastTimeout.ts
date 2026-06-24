import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const ethereumBroadcasterBroadcastTimeoutEvent = defineEvent(
  'EthereumBroadcaster.BroadcastTimeout',
  ethereumBroadcasterBroadcastTimeout,
);
