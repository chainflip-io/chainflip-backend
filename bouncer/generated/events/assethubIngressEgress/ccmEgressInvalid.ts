import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});

export const assethubIngressEgressCcmEgressInvalidEvent = defineEvent(
  'AssethubIngressEgress.CcmEgressInvalid',
  assethubIngressEgressCcmEgressInvalid,
);
