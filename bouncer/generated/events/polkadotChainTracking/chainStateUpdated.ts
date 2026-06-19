import { z } from 'zod';
import { cfChainsChainStatePolkadot } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStatePolkadot,
});

export const polkadotChainTrackingChainStateUpdatedEvent = defineEvent(
  'PolkadotChainTracking.ChainStateUpdated',
  polkadotChainTrackingChainStateUpdated,
);
