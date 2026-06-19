import { z } from 'zod';
import { accountId, cfPrimitivesAccountRole } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesAccountRoleRegistered = z.object({
  accountId,
  role: cfPrimitivesAccountRole,
});

export const accountRolesAccountRoleRegisteredEvent = defineEvent(
  'AccountRoles.AccountRoleRegistered',
  accountRolesAccountRoleRegistered,
);
