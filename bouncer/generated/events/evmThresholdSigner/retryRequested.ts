import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});
