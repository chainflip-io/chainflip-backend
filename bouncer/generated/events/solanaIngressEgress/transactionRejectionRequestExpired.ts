import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';

export const solanaIngressEgressTransactionRejectionRequestExpired = z.object({
  accountId,
  txId: z.tuple([hexString, numberOrHex]),
});
