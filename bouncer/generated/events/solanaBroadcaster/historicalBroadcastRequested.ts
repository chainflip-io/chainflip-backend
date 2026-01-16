import { z } from 'zod';

export const solanaBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});
