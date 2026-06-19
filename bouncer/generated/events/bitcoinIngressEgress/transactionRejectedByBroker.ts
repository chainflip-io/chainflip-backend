import { z } from 'zod';
import { cfChainsBtcUtxo } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsBtcUtxo,
});

export const bitcoinIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'BitcoinIngressEgress.TransactionRejectedByBroker',
  bitcoinIngressEgressTransactionRejectedByBroker,
);
