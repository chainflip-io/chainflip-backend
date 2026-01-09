import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerThresholdSignatureSuccess = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
});
