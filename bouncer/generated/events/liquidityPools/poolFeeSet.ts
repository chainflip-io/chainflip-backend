import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsPoolFeeSet = z.object({
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  feeHundredthPips: z.number(),
});

export const liquidityPoolsPoolFeeSetEvent = defineEvent(
  'LiquidityPools.PoolFeeSet',
  liquidityPoolsPoolFeeSet,
);
