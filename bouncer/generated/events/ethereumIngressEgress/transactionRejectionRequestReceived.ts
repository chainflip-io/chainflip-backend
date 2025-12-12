import { z } from 'zod';
import { accountId, hexString } from '../common';

export const ethereumIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: hexString,
  expiresAt: z.number(),
});
