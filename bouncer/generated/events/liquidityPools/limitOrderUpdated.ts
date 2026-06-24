import { z } from 'zod';
import {
  accountId,
  cfAmmCommonSide,
  cfPrimitivesChainsAssetsAnyAsset,
  cfTraitsLiquidityIncreaseOrDecreaseU128,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

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

export const liquidityPoolsLimitOrderUpdatedEvent = defineEvent(
  'LiquidityPools.LimitOrderUpdated',
  liquidityPoolsLimitOrderUpdated,
);
