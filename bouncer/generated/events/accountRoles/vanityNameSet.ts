import { z } from 'zod';
import { accountId, hexString } from '../common';

export const accountRolesVanityNameSet = z.object({ accountId, name: hexString });
