import { z } from 'zod';

export const reputationMissedHeartbeatPenaltyUpdated = z.object({
  newReputationPenalty: z.number(),
});
