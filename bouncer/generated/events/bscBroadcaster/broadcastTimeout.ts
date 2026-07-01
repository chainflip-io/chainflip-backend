import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const bscBroadcasterBroadcastTimeoutEvent = defineEvent(
  'BscBroadcaster.BroadcastTimeout',
  bscBroadcasterBroadcastTimeout,
);
