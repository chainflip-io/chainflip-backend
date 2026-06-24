import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingRefundEgressIgnored = z.object({
  swapRequestId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  reason: spRuntimeDispatchError,
});

export const swappingRefundEgressIgnoredEvent = defineEvent(
  'Swapping.RefundEgressIgnored',
  swappingRefundEgressIgnored,
);
