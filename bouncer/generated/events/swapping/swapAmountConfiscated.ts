import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const swappingSwapAmountConfiscated = z.object({
  swapRequestId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  totalAmount: numberOrHex,
  confiscatedAmount: numberOrHex,
});
