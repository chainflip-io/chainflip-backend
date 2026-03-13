import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';

export const bscIngressEgressFailedToBuildAllBatchCall = z.object({ error: cfChainsAllBatchError });
