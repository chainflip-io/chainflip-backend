import { z } from 'zod';
import { cfChainsChainStateTron } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateTron,
});

export const tronChainTrackingChainStateUpdatedEvent = defineEvent(
  'TronChainTracking.ChainStateUpdated',
  tronChainTrackingChainStateUpdated,
);
