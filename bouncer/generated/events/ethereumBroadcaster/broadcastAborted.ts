import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const ethereumBroadcasterBroadcastAbortedEvent = defineEvent(
  'EthereumBroadcaster.BroadcastAborted',
  ethereumBroadcasterBroadcastAborted,
);
