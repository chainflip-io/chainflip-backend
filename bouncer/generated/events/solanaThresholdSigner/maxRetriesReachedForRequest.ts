import { z } from 'zod';

export const solanaThresholdSignerMaxRetriesReachedForRequest = z.object({ requestId: z.number() });
