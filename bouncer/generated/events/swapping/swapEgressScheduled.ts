import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapEgressScheduled = z.object({
  swapRequestId: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  egressFee: z.tuple([numberOrHex, cfPrimitivesChainsAssetsAnyAsset]),
});

export const swappingSwapEgressScheduledEvent = defineEvent(
  'Swapping.SwapEgressScheduled',
  swappingSwapEgressScheduled,
);
