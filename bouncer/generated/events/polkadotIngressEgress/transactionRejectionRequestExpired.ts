import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: cfPrimitivesTxId,
});

export const polkadotIngressEgressTransactionRejectionRequestExpiredEvent = defineEvent(
  'PolkadotIngressEgress.TransactionRejectionRequestExpired',
  polkadotIngressEgressTransactionRejectionRequestExpired,
);
