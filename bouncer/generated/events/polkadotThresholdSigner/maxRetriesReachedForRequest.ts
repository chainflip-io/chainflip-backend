import { z } from 'zod';

export const polkadotThresholdSignerMaxRetriesReachedForRequest = z.object({
  requestId: z.number(),
});
