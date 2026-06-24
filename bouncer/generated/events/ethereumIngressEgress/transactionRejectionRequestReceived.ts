import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});

export const ethereumIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'EthereumIngressEgress.TransactionRejectionRequestReceived',
  ethereumIngressEgressTransactionRejectionRequestReceived,
);
