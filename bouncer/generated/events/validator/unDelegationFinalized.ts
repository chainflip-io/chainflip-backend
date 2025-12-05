import { z } from 'zod';
import { accountId } from '../common';

export const validatorUnDelegationFinalized = z.object({ delegator: accountId, epoch: z.number() });
