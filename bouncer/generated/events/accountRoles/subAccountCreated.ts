import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesSubAccountCreated = z.object({
  accountId,
  subAccountId: accountId,
  subAccountIndex: z.number(),
});

export const accountRolesSubAccountCreatedEvent = defineEvent(
  'AccountRoles.SubAccountCreated',
  accountRolesSubAccountCreated,
);
