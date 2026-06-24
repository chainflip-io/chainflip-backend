import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingRefundedOnChain = z.object({
  swapRequestId: numberOrHex,
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  refundFee: numberOrHex,
});

export const swappingRefundedOnChainEvent = defineEvent(
  'Swapping.RefundedOnChain',
  swappingRefundedOnChain,
);
