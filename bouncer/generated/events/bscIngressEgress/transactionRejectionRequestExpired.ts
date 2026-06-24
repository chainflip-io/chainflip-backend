import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: hexString,
});

export const bscIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'BscIngressEgress.TransactionRejectionRequestExpired',
  bscIngressEgressTransactionRejectionRequestExpired,
);
