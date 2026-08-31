import { z } from 'zod';
import { accountId, palletCfValidatorDelegationDelegatorRelations } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegationPlanUpdated = z.object({
  delegator: accountId,
  plan: palletCfValidatorDelegationDelegatorRelations,
});

export const validatorDelegationPlanUpdatedEvent = defineEvent(
  'Validator.DelegationPlanUpdated',
  validatorDelegationPlanUpdated,
);
