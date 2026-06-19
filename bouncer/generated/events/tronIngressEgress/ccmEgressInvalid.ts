import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});

export const tronIngressEgressCcmEgressInvalidEvent = defineEvent(
  'TronIngressEgress.CcmEgressInvalid',
  tronIngressEgressCcmEgressInvalid,
);
