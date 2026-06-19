import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});

export const tronIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'TronIngressEgress.TransactionRejectionRequestReceived',
  tronIngressEgressTransactionRejectionRequestReceived,
);
