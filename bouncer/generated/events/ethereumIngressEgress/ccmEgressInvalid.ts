import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';

export const ethereumIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});
