import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const polkadotBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'PolkadotBroadcaster.HistoricalBroadcastRequested',
  polkadotBroadcasterHistoricalBroadcastRequested,
);
