import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapAborted = z.object({
  swapId: numberOrHex,
  reason: palletCfSwappingSwapFailureReason,
});

export const swappingSwapAbortedEvent = defineEvent('Swapping.SwapAborted', swappingSwapAborted);
