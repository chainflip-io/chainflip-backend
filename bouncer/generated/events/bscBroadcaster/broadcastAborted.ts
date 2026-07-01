import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const bscBroadcasterBroadcastAbortedEvent = defineEvent(
  'BscBroadcaster.BroadcastAborted',
  bscBroadcasterBroadcastAborted,
);
