import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const bitcoinBroadcasterBroadcastAbortedEvent = defineEvent(
  'BitcoinBroadcaster.BroadcastAborted',
  bitcoinBroadcasterBroadcastAborted,
);
