import { z } from 'zod';
import { dispatchResult, numberOrHex } from '../common';

export const solanaThresholdSignerThresholdDispatchComplete = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  result: dispatchResult,
});
