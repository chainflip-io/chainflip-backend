import { z } from 'zod';
import { cfChainsEvmDepositDetails } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsEvmDepositDetails,
});

export const bscIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'BscIngressEgress.TransactionRejectedByBroker',
  bscIngressEgressTransactionRejectedByBroker,
);
