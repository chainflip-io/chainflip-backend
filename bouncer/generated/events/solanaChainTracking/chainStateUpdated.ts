import { z } from 'zod';
import { cfChainsChainStateSolana } from '../common';

export const solanaChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateSolana,
});
