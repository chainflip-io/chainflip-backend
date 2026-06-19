import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const solanaBroadcasterBroadcastAbortedEvent = defineEvent(
  'SolanaBroadcaster.BroadcastAborted',
  solanaBroadcasterBroadcastAborted,
);
