import { z } from 'zod';
import { cfAmmCommonAssetPair } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsPriceImpactLimitSet = z.object({
  assetPair: cfAmmCommonAssetPair,
  limit: z.number().nullish(),
});

export const liquidityPoolsPriceImpactLimitSetEvent = defineEvent(
  'LiquidityPools.PriceImpactLimitSet',
  liquidityPoolsPriceImpactLimitSet,
);
