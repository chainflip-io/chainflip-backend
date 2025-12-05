import { z } from 'zod';
import { palletCfPoolsAssetPair } from '../common';

export const liquidityPoolsPriceImpactLimitSet = z.object({
  assetPair: palletCfPoolsAssetPair,
  limit: z.number().nullish(),
});
