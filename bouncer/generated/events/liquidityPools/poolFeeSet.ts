import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset } from '../common';

export const liquidityPoolsPoolFeeSet = z.object({
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  feeHundredthPips: z.number(),
});
