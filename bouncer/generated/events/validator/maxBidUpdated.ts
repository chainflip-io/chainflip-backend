import { z } from 'zod';
import { accountId, palletCfValidatorDelegationChange } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorMaxBidUpdated = z.object({
  delegator: accountId,
  change: palletCfValidatorDelegationChange,
});

export const validatorMaxBidUpdatedEvent = defineEvent(
  'Validator.MaxBidUpdated',
  validatorMaxBidUpdated,
);
