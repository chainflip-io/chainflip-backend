import { z } from 'zod';
import {
  accountId,
  cfAmmCommonSide,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsLimitOrderFilled = z.object({
  lp: accountId,
  baseAsset: cfPrimitivesChainsAssetsAnyAsset,
  quoteAsset: cfPrimitivesChainsAssetsAnyAsset,
  side: cfAmmCommonSide,
  id: numberOrHex,
  tick: z.number(),
  soldAmount: numberOrHex,
  boughtAmount: numberOrHex,
  remainingAmount: numberOrHex,
});

export const liquidityPoolsLimitOrderFilledEvent = defineEvent(
  'LiquidityPools.LimitOrderFilled',
  liquidityPoolsLimitOrderFilled,
);
