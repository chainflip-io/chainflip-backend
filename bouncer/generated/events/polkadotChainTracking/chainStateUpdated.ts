import { z } from 'zod';
import { cfChainsChainStatePolkadot } from '../common';

export const polkadotChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStatePolkadot,
});
