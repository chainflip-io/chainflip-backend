import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const solanaBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'SolanaBroadcaster.HistoricalBroadcastRequested',
  solanaBroadcasterHistoricalBroadcastRequested,
);
