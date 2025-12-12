import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const solanaIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
