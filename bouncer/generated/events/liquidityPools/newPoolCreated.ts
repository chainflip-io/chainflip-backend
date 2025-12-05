import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const liquidityPoolsNewPoolCreated = z.object({
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  feeHundredthPips: z.number(),
  initialPrice: numberOrHex,
});
