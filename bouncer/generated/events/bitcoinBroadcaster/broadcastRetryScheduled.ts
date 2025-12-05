import { z } from 'zod';

export const bitcoinBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});
