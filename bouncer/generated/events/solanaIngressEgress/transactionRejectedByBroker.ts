import { z } from 'zod';
import { cfChainsSolVaultSwapOrDepositChannelId } from '../common';

export const solanaIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: cfChainsSolVaultSwapOrDepositChannelId,
});
