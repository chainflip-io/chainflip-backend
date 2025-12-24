import { z } from 'zod';
import { cfChainsChainStateAssethub } from '../common';

export const assethubChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateAssethub,
});
