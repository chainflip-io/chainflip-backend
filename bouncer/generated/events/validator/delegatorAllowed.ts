import { z } from 'zod';
import { accountId } from '../common';

export const validatorDelegatorAllowed = z.object({ delegator: accountId, operator: accountId });
