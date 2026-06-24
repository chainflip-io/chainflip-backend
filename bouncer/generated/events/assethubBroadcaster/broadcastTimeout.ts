import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const assethubBroadcasterBroadcastTimeoutEvent = defineEvent(
  'AssethubBroadcaster.BroadcastTimeout',
  assethubBroadcasterBroadcastTimeout,
);
