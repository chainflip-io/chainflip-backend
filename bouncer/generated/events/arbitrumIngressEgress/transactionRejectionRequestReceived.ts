import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});

export const arbitrumIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'ArbitrumIngressEgress.TransactionRejectionRequestReceived',
  arbitrumIngressEgressTransactionRejectionRequestReceived,
);
