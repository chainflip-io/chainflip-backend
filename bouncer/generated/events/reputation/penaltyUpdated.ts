import { z } from 'zod';
import { palletCfReputationPenalty, stateChainRuntimeChainflipOffencesOffence } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const reputationPenaltyUpdated = z.object({
  offence: stateChainRuntimeChainflipOffencesOffence,
  oldPenalty: palletCfReputationPenalty,
  newPenalty: palletCfReputationPenalty,
});

export const reputationPenaltyUpdatedEvent = defineEvent(
  'Reputation.PenaltyUpdated',
  reputationPenaltyUpdated,
);
