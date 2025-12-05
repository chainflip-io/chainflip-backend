import { z } from 'zod';
import { cfChainsChainStateArbitrum } from '../common';

export const arbitrumChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateArbitrum,
});
