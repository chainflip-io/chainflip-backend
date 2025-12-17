import { z } from 'zod';
import { palletCfReputationPenalty, stateChainRuntimeChainflipOffencesOffence } from '../common';

export const reputationPenaltyUpdated = z.object({
  offence: stateChainRuntimeChainflipOffencesOffence,
  oldPenalty: palletCfReputationPenalty,
  newPenalty: palletCfReputationPenalty,
});
