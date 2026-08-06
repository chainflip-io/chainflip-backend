import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorValidatorMaxBidUpdated = z.object({
  validator: accountId,
  maxBid: numberOrHex.nullish(),
});

export const validatorValidatorMaxBidUpdatedEvent = defineEvent(
  'Validator.ValidatorMaxBidUpdated',
  validatorValidatorMaxBidUpdated,
);
