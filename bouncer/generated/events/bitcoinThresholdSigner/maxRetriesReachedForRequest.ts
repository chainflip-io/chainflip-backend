import { z } from 'zod';

export const bitcoinThresholdSignerMaxRetriesReachedForRequest = z.object({
  requestId: z.number(),
});
