import { z } from 'zod';

export const historicalRootStored = z.object({ index: z.number() });
