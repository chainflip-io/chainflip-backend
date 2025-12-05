import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const arbitrumIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
