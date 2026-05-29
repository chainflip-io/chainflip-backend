import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const tronBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'TronBroadcaster.HistoricalBroadcastRequested',
  tronBroadcasterHistoricalBroadcastRequested,
);
