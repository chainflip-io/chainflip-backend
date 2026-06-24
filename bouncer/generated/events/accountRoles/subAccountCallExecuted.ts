import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesSubAccountCallExecuted = z.object({
  accountId,
  subAccountId: accountId,
  subAccountIndex: z.number(),
});

export const accountRolesSubAccountCallExecutedEvent = defineEvent(
  'AccountRoles.SubAccountCallExecuted',
  accountRolesSubAccountCallExecuted,
);
