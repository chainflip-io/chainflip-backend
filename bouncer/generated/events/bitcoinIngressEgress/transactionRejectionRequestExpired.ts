import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});

export const bitcoinIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'BitcoinIngressEgress.TransactionRejectionRequestExpired',
  bitcoinIngressEgressTransactionRejectionRequestExpired,
);
