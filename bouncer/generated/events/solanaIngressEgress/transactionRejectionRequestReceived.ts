import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: z.tuple([hexString, numberOrHex]),
  expiresAt: z.number(),
});

export const solanaIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'SolanaIngressEgress.TransactionRejectionRequestReceived',
  solanaIngressEgressTransactionRejectionRequestReceived,
);
