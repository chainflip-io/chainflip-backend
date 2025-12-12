import { z } from 'zod';
import { hexString } from '../common';

export const systemUpgradeAuthorized = z.object({ codeHash: hexString, checkVersion: z.boolean() });
