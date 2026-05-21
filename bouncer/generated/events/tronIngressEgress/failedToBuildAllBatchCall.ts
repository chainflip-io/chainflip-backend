import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const tronIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
