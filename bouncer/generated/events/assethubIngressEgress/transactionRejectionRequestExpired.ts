import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: cfPrimitivesTxId,
});

export const assethubIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectionRequestExpired',
  assethubIngressEgressTransactionRejectionRequestExpired,
);
