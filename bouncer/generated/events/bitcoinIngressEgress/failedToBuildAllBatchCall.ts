import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const bitcoinIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
