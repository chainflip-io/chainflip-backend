import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemUpgradeAuthorized = z.object({ codeHash: hexString, checkVersion: z.boolean() });

export const systemUpgradeAuthorizedEvent = defineEvent(
  'System.UpgradeAuthorized',
  systemUpgradeAuthorized,
);
