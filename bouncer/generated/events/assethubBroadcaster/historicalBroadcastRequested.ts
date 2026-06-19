import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const assethubBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'AssethubBroadcaster.HistoricalBroadcastRequested',
  assethubBroadcasterHistoricalBroadcastRequested,
);
