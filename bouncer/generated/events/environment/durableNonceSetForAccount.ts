import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentDurableNonceSetForAccount = z.object({
  nonceAccount: hexString,
  durableNonce: hexString,
});

export const environmentDurableNonceSetForAccountEvent = defineEvent(
  'Environment.DurableNonceSetForAccount',
  environmentDurableNonceSetForAccount,
);
