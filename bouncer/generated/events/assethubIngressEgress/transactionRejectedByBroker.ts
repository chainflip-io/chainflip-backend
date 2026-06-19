import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: z.number(),
});

export const assethubIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'AssethubIngressEgress.TransactionRejectedByBroker',
  assethubIngressEgressTransactionRejectedByBroker,
);
