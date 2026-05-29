import { z } from 'zod';
import { cfChainsSolVaultSwapOrDepositChannelId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsSolVaultSwapOrDepositChannelId,
});

export const solanaIngressEgressTransactionRejectedByBrokerEvent = defineEvent(
  'SolanaIngressEgress.TransactionRejectedByBroker',
  solanaIngressEgressTransactionRejectedByBroker,
);
