import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const arbitrumBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'ArbitrumBroadcaster.HistoricalBroadcastRequested',
  arbitrumBroadcasterHistoricalBroadcastRequested,
);
