import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const reputationAccrualRateUpdated = z.object({
  reputationPoints: z.number(),
  numberOfBlocks: z.number(),
});

export const reputationAccrualRateUpdatedEvent = defineEvent(
  'Reputation.AccrualRateUpdated',
  reputationAccrualRateUpdated,
);
