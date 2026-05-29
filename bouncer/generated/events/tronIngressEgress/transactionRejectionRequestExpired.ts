import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});

export const tronIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'TronIngressEgress.TransactionRejectionRequestExpired',
  tronIngressEgressTransactionRejectionRequestExpired,
);
