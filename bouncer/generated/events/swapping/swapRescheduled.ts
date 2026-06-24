import { z } from 'zod';
import { numberOrHex, palletCfSwappingSwapFailureReason } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapRescheduled = z.object({
  swapId: numberOrHex,
  executeAt: z.number(),
  reason: palletCfSwappingSwapFailureReason,
});

export const swappingSwapRescheduledEvent = defineEvent(
  'Swapping.SwapRescheduled',
  swappingSwapRescheduled,
);
