import { z } from 'zod';
import {
  accountId,
  cfAmmCommonPoolPairsMap,
  cfPrimitivesChainsAssetsAnyAsset,
  cfTraitsLiquidityIncreaseOrDecreaseRangeOrderChange,
  numberOrHex,
} from '../common';

export const liquidityPoolsRangeOrderUpdated = z.object({
  lp: accountId,
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  id: numberOrHex,
  tickRange: z.object({ start: z.number(), end: z.number() }),
  sizeChange: cfTraitsLiquidityIncreaseOrDecreaseRangeOrderChange.nullish(),
  liquidityTotal: numberOrHex,
  collectedFees: cfAmmCommonPoolPairsMap,
});
