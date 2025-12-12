import { z } from 'zod';

export const polkadotIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: z.number(),
});
