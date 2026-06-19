import { z } from 'zod';
import { cfChainsChainStateEthereum } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateEthereum,
});

export const ethereumChainTrackingChainStateUpdatedEvent = defineEvent(
  'EthereumChainTracking.ChainStateUpdated',
  ethereumChainTrackingChainStateUpdated,
);
