import { z } from 'zod';
import { accountId } from '../common';

export const validatorDelegatorBlocked = z.object({ delegator: accountId, operator: accountId });
