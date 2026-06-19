import { z } from 'zod';
import { accountId, stateChainRuntimeChainflipOffencesOffence } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const reputationOffencePenalty = z.object({
  offender: accountId,
  offence: stateChainRuntimeChainflipOffencesOffence,
  penalty: z.number(),
});

export const reputationOffencePenaltyEvent = defineEvent(
  'Reputation.OffencePenalty',
  reputationOffencePenalty,
);
