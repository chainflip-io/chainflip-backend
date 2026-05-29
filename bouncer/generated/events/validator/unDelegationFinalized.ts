import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorUnDelegationFinalized = z.object({ delegator: accountId, epoch: z.number() });

export const validatorUnDelegationFinalizedEvent = defineEvent(
  'Validator.UnDelegationFinalized',
  validatorUnDelegationFinalized,
);
