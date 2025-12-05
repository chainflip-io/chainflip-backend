import { z } from 'zod';

export const solanaThresholdSignerSignersUnavailable = z.object({
  requestId: z.number(),
  attemptCount: z.number(),
});
