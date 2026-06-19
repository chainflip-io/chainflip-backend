import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const bitcoinBroadcasterBroadcastTimeoutEvent = defineEvent(
  'BitcoinBroadcaster.BroadcastTimeout',
  bitcoinBroadcasterBroadcastTimeout,
);
