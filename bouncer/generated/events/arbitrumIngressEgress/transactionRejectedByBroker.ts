import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});

export const arbitrumIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'ArbitrumIngressEgress.TransactionRejectedByBroker',
  arbitrumIngressEgressTransactionRejectedByBroker,
);
