import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const solanaThresholdSignerKeygenRequest = z.object({
  ceremonyId: numberOrHex,
  participants: z.array(accountId),
  epochIndex: z.number(),
});
