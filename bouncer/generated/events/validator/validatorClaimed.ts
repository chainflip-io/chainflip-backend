import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorValidatorClaimed = z.object({ validator: accountId, operator: accountId });

export const validatorValidatorClaimedEvent = defineEvent(
  'Validator.ValidatorClaimed',
  validatorValidatorClaimed,
);
