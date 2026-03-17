import { z } from 'zod';
import { accountId, numberOrHex, palletCfSwappingSwapRequestCompletionReason } from '../common';

export const swappingSwapRequestCompleted = z.object({
  swapRequestId: numberOrHex,
  reason: palletCfSwappingSwapRequestCompletionReason,
  brokerFeeSwaps: z.array(z.tuple([accountId, numberOrHex])),
});
