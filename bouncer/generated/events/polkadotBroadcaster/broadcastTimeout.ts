import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterBroadcastTimeout = z.object({ broadcastId: z.number() });

export const polkadotBroadcasterBroadcastTimeoutEvent = defineEvent(
  'PolkadotBroadcaster.BroadcastTimeout',
  polkadotBroadcasterBroadcastTimeout,
);
