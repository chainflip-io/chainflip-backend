import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});

export const bitcoinBroadcasterHistoricalBroadcastRequestedEvent = defineEvent(
  'BitcoinBroadcaster.HistoricalBroadcastRequested',
  bitcoinBroadcasterHistoricalBroadcastRequested,
);
