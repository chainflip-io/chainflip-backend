import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});

export const ethereumIngressEgressCcmEgressInvalidEvent = defineEvent(
  'EthereumIngressEgress.CcmEgressInvalid',
  ethereumIngressEgressCcmEgressInvalid,
);
