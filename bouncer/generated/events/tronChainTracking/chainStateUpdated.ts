import { z } from 'zod';
import { cfChainsChainStateTron } from '../common';

export const tronChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateTron,
});
