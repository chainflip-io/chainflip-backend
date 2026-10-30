import { z } from 'zod';
import { accountId, palletCfValidatorDelegationDelegationPlan } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegationPlanUpdated = z.object({
  delegator: accountId,
  plan: palletCfValidatorDelegationDelegationPlan,
});

export const validatorDelegationPlanUpdatedEvent = defineEvent(
  'Validator.DelegationPlanUpdated',
  validatorDelegationPlanUpdated,
);
