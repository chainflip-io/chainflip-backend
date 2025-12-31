import { z } from 'zod';

export const arbitrumBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});
