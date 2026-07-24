import { z } from 'zod';
import { cfPrimitivesChainsForeignChain, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSentFlipToGateway = z.object({
  amount: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
});

export const swappingSentFlipToGatewayEvent = defineEvent(
  'Swapping.SentFlipToGateway',
  swappingSentFlipToGateway,
);
