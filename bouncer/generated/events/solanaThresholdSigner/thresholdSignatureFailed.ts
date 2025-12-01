import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const solanaThresholdSignerThresholdSignatureFailed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  offenders: z.array(accountId),
});
