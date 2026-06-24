import { z } from 'zod';
import { accountId, cfPrimitivesAccountRole } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesAccountRoleDeregistered = z.object({
  accountId,
  role: cfPrimitivesAccountRole,
});

export const accountRolesAccountRoleDeregisteredEvent = defineEvent(
  'AccountRoles.AccountRoleDeregistered',
  accountRolesAccountRoleDeregistered,
);
