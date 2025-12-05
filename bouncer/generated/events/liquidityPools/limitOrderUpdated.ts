import { z } from 'zod';
import {
  accountId,
  cfAmmCommonSide,
  cfPrimitivesChainsAssetsAnyAsset,
  cfTraitsLiquidityIncreaseOrDecreaseU128,
  numberOrHex,
} from '../common';

export const liquidityPoolsLimitOrderUpdated = z.object({
  lp: accountId,
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  side: cfAmmCommonSide,
  id: numberOrHex,
  tick: z.number(),
  sellAmountChange: cfTraitsLiquidityIncreaseOrDecreaseU128.nullish(),
  sellAmountTotal: numberOrHex,
  collectedFees: numberOrHex,
  boughtAmount: numberOrHex,
});
