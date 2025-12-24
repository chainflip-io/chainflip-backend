import { z } from 'zod';
import { accountId } from '../common';

export const accountRolesSubAccountCallExecuted = z.object({
  accountId,
  subAccountId: accountId,
  subAccountIndex: z.number(),
});
