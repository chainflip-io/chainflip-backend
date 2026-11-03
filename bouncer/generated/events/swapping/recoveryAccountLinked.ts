import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingRecoveryAccountLinked = z.object({
  broker: accountId,
  recoveryAccount: accountId,
});

export const swappingRecoveryAccountLinkedEvent = defineEvent(
  'Swapping.RecoveryAccountLinked',
  swappingRecoveryAccountLinked,
);
