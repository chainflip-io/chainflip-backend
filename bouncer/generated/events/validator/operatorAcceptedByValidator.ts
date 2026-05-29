import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorOperatorAcceptedByValidator = z.object({
  validator: accountId,
  operator: accountId,
});

export const validatorOperatorAcceptedByValidatorEvent = defineEvent(
  'Validator.OperatorAcceptedByValidator',
  validatorOperatorAcceptedByValidator,
);
