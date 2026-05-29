import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorValidatorRemovedFromOperator = z.object({
  validator: accountId,
  operator: accountId,
});

export const validatorValidatorRemovedFromOperatorEvent = defineEvent(
  'Validator.ValidatorRemovedFromOperator',
  validatorValidatorRemovedFromOperator,
);
