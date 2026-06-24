import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});

export const arbitrumIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'ArbitrumIngressEgress.TransactionRejectionRequestExpired',
  arbitrumIngressEgressTransactionRejectionRequestExpired,
);
