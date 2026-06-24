import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const accountRolesVanityNameSet = z.object({ accountId, name: hexString });

export const accountRolesVanityNameSetEvent = defineEvent(
  'AccountRoles.VanityNameSet',
  accountRolesVanityNameSet,
);
