import { z } from 'zod';
import { accountId, palletCfValidatorDelegationDelegationPlanU128 } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegationPlanUpdated = z.object({
  delegator: accountId,
  plan: palletCfValidatorDelegationDelegationPlanU128,
});

export const validatorDelegationPlanUpdatedEvent = defineEvent(
  'Validator.DelegationPlanUpdated',
  validatorDelegationPlanUpdated,
);
