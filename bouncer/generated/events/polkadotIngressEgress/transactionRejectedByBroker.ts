import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: z.number(),
});

export const polkadotIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'PolkadotIngressEgress.TransactionRejectedByBroker',
  polkadotIngressEgressTransactionRejectedByBroker,
);
