import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsNewPoolCreated = z.object({
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  feeHundredthPips: z.number(),
  initialPrice: numberOrHex,
});

export const liquidityPoolsNewPoolCreatedEvent = defineEvent(
  'LiquidityPools.NewPoolCreated',
  liquidityPoolsNewPoolCreated,
);
