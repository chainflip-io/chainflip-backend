import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const swappingRefundedOnChain = z.object({
  swapRequestId: numberOrHex,
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
});
