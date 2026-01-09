import { z } from 'zod';
import { accountId, hexString } from '../common';

export const witnesserReportedWitnessingFailures = z.object({
  callHash: hexString,
  blockNumber: z.number(),
  accounts: z.array(accountId),
});
