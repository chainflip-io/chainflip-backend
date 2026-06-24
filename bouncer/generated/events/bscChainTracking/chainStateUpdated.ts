import { z } from 'zod';
import { cfChainsChainStateBsc } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscChainTrackingChainStateUpdated = z.object({ newChainState: cfChainsChainStateBsc });

export const bscChainTrackingChainStateUpdatedEvent = defineEvent(
  'BscChainTracking.ChainStateUpdated',
  bscChainTrackingChainStateUpdated,
);
