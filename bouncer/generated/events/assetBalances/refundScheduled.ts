import { z } from 'zod';
import {
  cfChainsAddressForeignChainAddress,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';

export const assetBalancesRefundScheduled = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  destination: cfChainsAddressForeignChainAddress,
  amount: numberOrHex,
});
