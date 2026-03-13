import { z } from 'zod';

export const bscBroadcasterBroadcastRetryScheduled = z.object({
  broadcastId: z.number(),
  retryBlock: z.number(),
});
