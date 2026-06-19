import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorUndelegated = z.object({
  delegator: accountId,
  operator: accountId,
  maxBid: numberOrHex,
});

export const validatorUndelegatedEvent = defineEvent('Validator.Undelegated', validatorUndelegated);
