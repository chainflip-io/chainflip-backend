import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerRetryRequested = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});
