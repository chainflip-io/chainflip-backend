import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const tronBroadcasterBroadcastTimeoutEvent = defineEvent(
  'TronBroadcaster.BroadcastTimeout',
  tronBroadcasterBroadcastTimeout,
);
