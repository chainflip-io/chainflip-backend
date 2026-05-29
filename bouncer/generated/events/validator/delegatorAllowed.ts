import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorDelegatorAllowed = z.object({ delegator: accountId, operator: accountId });

export const validatorDelegatorAllowedEvent = defineEvent(
  'Validator.DelegatorAllowed',
  validatorDelegatorAllowed,
);
