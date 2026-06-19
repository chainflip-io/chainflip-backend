import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});

export const tronIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'TronIngressEgress.TransactionRejectedByBroker',
  tronIngressEgressTransactionRejectedByBroker,
);
