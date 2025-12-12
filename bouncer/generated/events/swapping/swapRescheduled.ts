import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapFailureReason } from '../common';

export const swappingSwapRescheduled = z.object({
  swapId: numberOrHex,
  executeAt: z.number(),
  reason: palletCfSwappingSwapFailureReason,
});
