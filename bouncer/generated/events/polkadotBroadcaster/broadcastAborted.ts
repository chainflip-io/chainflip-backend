import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterBroadcastAborted = z.object({ broadcastId: z.number() });

export const polkadotBroadcasterBroadcastAbortedEvent = defineEvent(
  'PolkadotBroadcaster.BroadcastAborted',
  polkadotBroadcasterBroadcastAborted,
);
