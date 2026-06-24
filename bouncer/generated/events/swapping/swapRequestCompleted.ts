import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapRequestCompletionReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapRequestCompleted = z.object({
  swapRequestId: numberOrHex,
  reason: palletCfSwappingSwapRequestCompletionReason,
});

export const swappingSwapRequestCompletedEvent = defineEvent(
  'Swapping.SwapRequestCompleted',
  swappingSwapRequestCompleted,
);
