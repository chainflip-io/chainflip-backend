import { z } from 'zod';

export const evmThresholdSignerMaxRetriesReachedForRequest = z.object({ requestId: z.number() });
