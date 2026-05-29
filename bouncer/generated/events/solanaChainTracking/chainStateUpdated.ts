import { z } from 'zod';
import { cfChainsChainStateSolana } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateSolana,
});

export const solanaChainTrackingChainStateUpdatedEvent = defineEvent(
  'SolanaChainTracking.ChainStateUpdated',
  solanaChainTrackingChainStateUpdated,
);
