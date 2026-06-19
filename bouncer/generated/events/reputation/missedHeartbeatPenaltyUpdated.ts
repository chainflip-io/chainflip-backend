import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const reputationMissedHeartbeatPenaltyUpdated = z.object({
  newReputationPenalty: z.number(),
});

export const reputationMissedHeartbeatPenaltyUpdatedEvent = defineEvent(
  'Reputation.MissedHeartbeatPenaltyUpdated',
  reputationMissedHeartbeatPenaltyUpdated,
);
