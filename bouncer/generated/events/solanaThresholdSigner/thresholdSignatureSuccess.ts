import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerThresholdSignatureSuccess = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});
