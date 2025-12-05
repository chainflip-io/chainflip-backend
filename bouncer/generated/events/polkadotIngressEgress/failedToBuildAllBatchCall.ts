import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const polkadotIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});
