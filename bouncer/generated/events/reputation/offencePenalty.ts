import { z } from 'zod';
import { accountId, stateChainRuntimeChainflipOffencesOffence } from '../common';

export const reputationOffencePenalty = z.object({
  offender: accountId,
  offence: stateChainRuntimeChainflipOffencesOffence,
  penalty: z.number(),
});
