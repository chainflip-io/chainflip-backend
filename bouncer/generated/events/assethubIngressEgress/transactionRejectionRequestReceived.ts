import { z } from 'zod';
import { accountId, cfPrimitivesTxId } from '../common';

export const assethubIngressEgressTransactionRejectionRequestReceived = z.object({
  accountId,
  txId: cfPrimitivesTxId,
  expiresAt: z.number(),
});
