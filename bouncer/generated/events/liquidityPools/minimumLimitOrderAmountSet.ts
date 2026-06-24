import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsMinimumLimitOrderAmountSet = z.object({
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
});

export const liquidityPoolsMinimumLimitOrderAmountSetEvent = defineEvent(
  'LiquidityPools.MinimumLimitOrderAmountSet',
  liquidityPoolsMinimumLimitOrderAmountSet,
);
