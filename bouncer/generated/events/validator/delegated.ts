import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegated = z.object({
  delegator: accountId,
  operator: accountId,
  maxBid: numberOrHex,
});

export const validatorDelegatedEvent = defineEvent('Validator.Delegated', validatorDelegated);
