import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const assethubBroadcasterBroadcastAbortedEvent = defineEvent(
  'AssethubBroadcaster.BroadcastAborted',
  assethubBroadcasterBroadcastAborted,
);
