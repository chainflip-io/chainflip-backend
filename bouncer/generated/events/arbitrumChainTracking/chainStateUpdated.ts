import { z } from 'zod';
import { cfChainsChainStateArbitrum } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateArbitrum,
});

export const arbitrumChainTrackingChainStateUpdatedEvent = defineEvent(
  'ArbitrumChainTracking.ChainStateUpdated',
  arbitrumChainTrackingChainStateUpdated,
);
