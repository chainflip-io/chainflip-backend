import { z } from 'zod';
import { cfTraitsSwappingSwapType, numberOrHex } from '../common';

export const swappingSwapScheduled = z.object({
  swapRequestId: numberOrHex,
  swapId: numberOrHex,
  inputAmount: numberOrHex,
  swapType: cfTraitsSwappingSwapType,
  executeAt: z.number(),
});
