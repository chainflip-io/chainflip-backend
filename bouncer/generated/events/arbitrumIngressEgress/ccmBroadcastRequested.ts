import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';

export const arbitrumIngressEgressCcmBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});
