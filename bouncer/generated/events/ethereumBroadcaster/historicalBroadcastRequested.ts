import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const ethereumBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'EthereumBroadcaster.HistoricalBroadcastRequested',
  ethereumBroadcasterHistoricalBroadcastRequested,
);
