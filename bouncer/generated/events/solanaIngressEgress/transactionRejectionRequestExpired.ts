import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: z.tuple([hexString, numberOrHex]),
});

export const solanaIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'SolanaIngressEgress.TransactionRejectionRequestExpired',
  solanaIngressEgressTransactionRejectionRequestExpired,
);
