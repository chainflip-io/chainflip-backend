import { z } from 'zod';

export const tronBroadcasterHistoricalBroadcastRequested = z.object({
  broadcastId: z.number(),
  thresholdSignatureRequestId: z.number(),
  epochIndex: z.number(),
});
