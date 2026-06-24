import { z } from 'zod';
import { cfTraitsSwappingSwapType, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapScheduled = z.object({
  swapRequestId: numberOrHex,
  swapId: numberOrHex,
  inputAmount: numberOrHex,
  swapType: cfTraitsSwappingSwapType,
  executeAt: z.number(),
});

export const swappingSwapScheduledEvent = defineEvent(
  'Swapping.SwapScheduled',
  swappingSwapScheduled,
);
