import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const bscBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'BscBroadcaster.HistoricalBroadcastRequested',
  bscBroadcasterHistoricalBroadcastRequested,
);
