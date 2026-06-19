import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, cfPrimitivesSwapLeg, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingBatchSwapFailed = z.object({
  asset: cfPrimitivesChainsAssetsAnyAsset,
  direction: cfPrimitivesSwapLeg,
  amount: numberOrHex,
});

export const swappingBatchSwapFailedEvent = defineEvent(
  'Swapping.BatchSwapFailed',
  swappingBatchSwapFailed,
);
