import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const ethereumIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
