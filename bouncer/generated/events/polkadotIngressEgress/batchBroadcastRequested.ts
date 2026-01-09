import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';

export const polkadotIngressEgressBatchBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressIds: z.array(z.tuple([cfPrimitivesChainsForeignChain, numberOrHex])),
});
