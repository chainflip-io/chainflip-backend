import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegatorBlocked = z.object({ delegator: accountId, operator: accountId });

export const validatorDelegatorBlockedEvent = defineEvent(
  'Validator.DelegatorBlocked',
  validatorDelegatorBlocked,
);
