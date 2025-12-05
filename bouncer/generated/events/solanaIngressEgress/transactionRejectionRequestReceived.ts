import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';

export const solanaIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: z.tuple([hexString, numberOrHex]),
  expiresAt: z.number(),
});
