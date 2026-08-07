import { z } from 'zod';
import { cfPrimitivesTxId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfPrimitivesTxId,
});

export const assethubIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectedByBroker',
  assethubIngressEgressTransactionRejectedByBroker,
);
