import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});

export const ethereumIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'EthereumIngressEgress.TransactionRejectionRequestExpired',
  ethereumIngressEgressTransactionRejectionRequestExpired,
);
