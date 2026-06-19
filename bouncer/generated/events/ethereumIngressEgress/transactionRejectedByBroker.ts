import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});

export const ethereumIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'EthereumIngressEgress.TransactionRejectedByBroker',
  ethereumIngressEgressTransactionRejectedByBroker,
);
