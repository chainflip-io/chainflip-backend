import { z } from 'zod';

export const polkadotIngressEgressFailedForeignChainCallExpired = z.object({
  broadcastId: z.number(),
});
