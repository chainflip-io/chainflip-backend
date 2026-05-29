import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const solanaBroadcasterBroadcastTimeoutEvent = defineEvent(
  'SolanaBroadcaster.BroadcastTimeout',
  solanaBroadcasterBroadcastTimeout,
);
