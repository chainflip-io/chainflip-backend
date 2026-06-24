import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: cfPrimitivesTxId,
  expiresAt: z.number(),
});

export const polkadotIngressEgressTransactionRejectionRequestReceivedEvent = defineEvent(
  'PolkadotIngressEgress.TransactionRejectionRequestReceived',
  polkadotIngressEgressTransactionRejectionRequestReceived,
);
