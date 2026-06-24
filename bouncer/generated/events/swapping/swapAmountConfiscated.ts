import { z } from 'zod';
import { cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapAmountConfiscated = z.object({
  swapRequestId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  totalAmount: numberOrHex,
  confiscatedAmount: numberOrHex,
});

export const swappingSwapAmountConfiscatedEvent = defineEvent(
  'Swapping.SwapAmountConfiscated',
  swappingSwapAmountConfiscated,
);
