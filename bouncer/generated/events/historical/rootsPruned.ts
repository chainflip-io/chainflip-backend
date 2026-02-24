import { z } from 'zod';

export const historicalRootsPruned = z.object({ upTo: z.number() });
