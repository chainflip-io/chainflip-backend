import { z } from 'zod';
import { accountId, cfPrimitivesAccountRole } from '../common';

export const accountRolesAccountRoleDeregistered = z.object({
  accountId,
  role: cfPrimitivesAccountRole,
});
