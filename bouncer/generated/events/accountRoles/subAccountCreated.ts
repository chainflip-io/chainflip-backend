import { z } from 'zod';
import { accountId } from '../common';

export const accountRolesSubAccountCreated = z.object({
  accountId,
  subAccountId: accountId,
  subAccountIndex: z.number(),
});
