import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const flipFlipDistributed = z.object({ amount: z.array(z.tuple([accountId, numberOrHex])) });
