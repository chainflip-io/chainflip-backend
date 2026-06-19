import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: cfPrimitivesTxId,
  expiresAt: z.number(),
});

export const assethubIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectionRequestReceived',
  assethubIngressEgressTransactionRejectionRequestReceived,
);
