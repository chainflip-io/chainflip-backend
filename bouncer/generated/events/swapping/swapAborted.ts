import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapFailureReason } from '../common';

export const swappingSwapAborted = z.object({
  swapId: numberOrHex,
  reason: palletCfSwappingSwapFailureReason,
});
