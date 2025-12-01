import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapRequestCompletionReason } from '../common';

export const swappingSwapRequestCompleted = z.object({
  swapRequestId: numberOrHex,
  reason: palletCfSwappingSwapRequestCompletionReason,
});
