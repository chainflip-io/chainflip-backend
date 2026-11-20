import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesSubAccountsEnabledSet = z.object({ enabled: z.boolean() });

export const accountRolesSubAccountsEnabledSetEvent = defineEvent(
  'AccountRoles.SubAccountsEnabledSet',
  accountRolesSubAccountsEnabledSet,
);
