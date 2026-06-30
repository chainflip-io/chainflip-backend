import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});

export const bscIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'BscIngressEgress.TransactionRejectionRequestReceived',
  bscIngressEgressTransactionRejectionRequestReceived,
);
