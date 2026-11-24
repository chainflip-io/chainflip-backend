import { z } from 'zod';
import {
  accountId,
  palletCfValidatorDelegationDelegationChangeReason,
  palletCfValidatorDelegationDelegationPlanDetails,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegationPlanUpdated = z.object({
  delegator: accountId,
  oldPlan: palletCfValidatorDelegationDelegationPlanDetails.nullish(),
  newPlan: palletCfValidatorDelegationDelegationPlanDetails.nullish(),
  reason: palletCfValidatorDelegationDelegationChangeReason,
});

export const validatorDelegationPlanUpdatedEvent = defineEvent(
  'Validator.DelegationPlanUpdated',
  validatorDelegationPlanUpdated,
);
