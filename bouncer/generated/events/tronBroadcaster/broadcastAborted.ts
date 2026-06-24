import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const tronBroadcasterBroadcastAbortedEvent = defineEvent(
  'TronBroadcaster.BroadcastAborted',
  tronBroadcasterBroadcastAborted,
);
