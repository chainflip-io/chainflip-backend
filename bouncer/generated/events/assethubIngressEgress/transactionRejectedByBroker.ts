import { z } from 'zod';

export const assethubIngressEgressTransactionRejectedByBroker = z.object({
  broadcastId: z.number(),
  txId: z.number(),
});
