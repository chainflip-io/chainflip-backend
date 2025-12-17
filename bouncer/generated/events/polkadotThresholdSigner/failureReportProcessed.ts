import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const polkadotThresholdSignerFailureReportProcessed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  reporterId: accountId,
});
