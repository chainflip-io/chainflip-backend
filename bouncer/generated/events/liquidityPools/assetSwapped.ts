import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsAssetSwapped = z.object({
  from: cfPrimitivesChainsAssetsAnyAsset,
  to: cfPrimitivesChainsAssetsAnyAsset,
  inputAmount: numberOrHex,
  outputAmount: numberOrHex,
});

export const liquidityPoolsAssetSwappedEvent = defineEvent(
  'LiquidityPools.AssetSwapped',
  liquidityPoolsAssetSwapped,
);
