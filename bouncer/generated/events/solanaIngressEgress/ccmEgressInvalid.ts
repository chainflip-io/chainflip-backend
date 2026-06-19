import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});

export const solanaIngressEgressCcmEgressInvalidEvent = defineEvent(
  'SolanaIngressEgress.CcmEgressInvalid',
  solanaIngressEgressCcmEgressInvalid,
);
