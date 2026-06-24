import { z } from 'zod';
import {
  cfChainsExecutexSwapAndCallError,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressCcmEgressInvalid = z.object({
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  error: cfChainsExecutexSwapAndCallError,
});

export const polkadotIngressEgressCcmEgressInvalidEvent = defineEvent(
  'PolkadotIngressEgress.CcmEgressInvalid',
  polkadotIngressEgressCcmEgressInvalid,
);
