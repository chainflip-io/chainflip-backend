import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingRefundEgressScheduled = z.object({
  swapRequestId: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  egressFee: z.tuple([numberOrHex, cfPrimitivesChainsAssetsAnyAsset]),
  refundFee: numberOrHex,
});

export const swappingRefundEgressScheduledEvent = defineEvent(
  'Swapping.RefundEgressScheduled',
  swappingRefundEgressScheduled,
);
