import { z } from 'zod';
import { cfChainsChainStateAssethub } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateAssethub,
});

export const assethubChainTrackingChainStateUpdatedEvent = defineEvent(
  'AssethubChainTracking.ChainStateUpdated',
  assethubChainTrackingChainStateUpdated,
);
