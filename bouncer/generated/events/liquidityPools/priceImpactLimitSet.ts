import { z } from 'zod';
import { cfAmmCommonAssetPair } from '../common';

export const liquidityPoolsPriceImpactLimitSet = z.object({
  assetPair: cfAmmCommonAssetPair,
  limit: z.number().nullish(),
});
