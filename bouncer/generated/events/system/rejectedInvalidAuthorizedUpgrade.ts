import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemRejectedInvalidAuthorizedUpgrade = z.object({
  codeHash: hexString,
  error: spRuntimeDispatchError,
});

export const systemRejectedInvalidAuthorizedUpgradeEvent = defineEvent(
  'System.RejectedInvalidAuthorizedUpgrade',
  systemRejectedInvalidAuthorizedUpgrade,
);
