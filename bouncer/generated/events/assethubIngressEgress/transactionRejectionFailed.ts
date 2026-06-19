import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectionFailed = z.object({ txId: z.number() });

export const assethubIngressEgressTransactionRejectionFailedEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectionFailed',
  assethubIngressEgressTransactionRejectionFailed,
);
