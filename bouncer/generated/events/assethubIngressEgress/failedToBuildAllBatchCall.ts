import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const assethubIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
