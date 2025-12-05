import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';

export const polkadotIngressEgressCcmBroadcastRequested = z.object({
  broadcastId: z.number(),
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});
