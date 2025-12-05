import { z } from 'zod';
import { accountId, cfPrimitivesAccountRole } from '../common';

export const accountRolesAccountRoleRegistered = z.object({
  accountId,
  role: cfPrimitivesAccountRole,
});
